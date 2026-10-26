//! Navigation with a NavHost (docs/guide/kotlin-to-rust.md, "Navigation").

use compose::foundation::layout::*;
use compose::foundation::{LazyColumn, LazyListScope};
use compose::material3::*;
use compose::navigation::*;
use compose::runtime::*;

// ANCHOR: screens
/// The app's destinations: Navigation Compose's routes, as an enum (`ComposeArg`, so a
/// composable can take one).
#[derive(Clone, Copy, PartialEq, Debug, ComposeArg)]
pub enum Screen {
    Inbox,
    Message(u32),
    Settings,
}

/// Saved as a number, so the back stack survives the activity's recreation.
impl Saveable for Screen {
    fn save(&self) -> SavedValue {
        SavedValue::Int(match *self {
            Screen::Inbox => -1,
            Screen::Settings => -2,
            Screen::Message(id) => i64::from(id),
        })
    }

    fn restore(saved: &SavedValue) -> Option<Screen> {
        match *saved {
            SavedValue::Int(-1) => Some(Screen::Inbox),
            SavedValue::Int(-2) => Some(Screen::Settings),
            SavedValue::Int(id) => u32::try_from(id).ok().map(Screen::Message),
            _ => None,
        }
    }
}
// ANCHOR_END: screens

// ANCHOR: app
#[composable]
pub fn App() {
    // The back stack, starting at the inbox: saved, so it survives the activity's recreation.
    let nav_controller = remember_nav_controller(Screen::Inbox);
    // Shows the top entry; back pops the stack, and leaves the app at the inbox.
    NavHost!(nav_controller, move |screen: Screen| match screen {
        Screen::Inbox => Inbox!(
            on_open = move |id: u32| nav_controller.navigate(Screen::Message(id)),
            on_settings = move || nav_controller.navigate(Screen::Settings)
        ),
        Screen::Message(id) => MessageScreen!(id),
        Screen::Settings => SettingsScreen!(),
    });
}
// ANCHOR_END: app

// ANCHOR: redirect
/// Sends the user to `to` as soon as it shows, in place of itself. Navigating writes state, so
/// it goes in an effect or an event handler, never in a composable's body (debug builds panic).
#[composable]
pub fn Redirect(nav_controller: NavController<Screen>, to: Screen) {
    LaunchedEffect((), async move {
        nav_controller.navigate_with(
            to,
            NavOptions { pop_up_to: Some(Screen::Inbox), ..NavOptions::default() },
        );
    });
}
// ANCHOR_END: redirect

#[composable]
pub fn Inbox(on_open: impl Fn(u32) + 'static, on_settings: impl Fn() + 'static) {
    Column!({
        TextButton!(on_click = move || on_settings(), {
            Text!("Settings");
        });
        LazyColumn!(|scope: LazyListScope| {
            scope.items_keyed(
                20,
                |index| index as u32,
                move |index| {
                    let id = index as u32;
                    ClickableListItem!(move || on_open(id), {
                        Text!("Message {}", id);
                    });
                },
            );
        });
    });
}

#[composable]
pub fn MessageScreen(id: u32) {
    let starred = remember_saveable(|| mutable_state_of(false));
    Row!(vertical_alignment = Alignment::CENTER_VERTICALLY, {
        Text!("Message {}", id);
        IconToggleButton!(starred.get(), move |checked| starred.set(checked), {
            Icon!(icons::filled::star(), Some("Star"));
        });
    });
}

#[composable]
pub fn SettingsScreen() {
    Text!("Settings");
}
