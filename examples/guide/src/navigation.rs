//! Navigation without a NavHost (docs/guide/kotlin-to-rust.md, "Navigation").

use compose::foundation::layout::*;
use compose::foundation::{LazyColumn, LazyListScope};
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

// ANCHOR: screens
/// The app's destinations: Navigation Compose's routes, as an enum.
#[derive(Clone, Copy, PartialEq, Debug)]
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
    // The back stack: a saved list of destinations, the current one last.
    let back_stack = remember_saveable(|| mutable_state_of(vec![Screen::Inbox]));
    let navigate = callback(move |screen: Screen| back_stack.edit(|stack| stack.push(screen)));
    // Back pops the stack while there's somewhere to go back to; at the root, back leaves the
    // app (or goes to the Kotlin UI around it).
    BackHandler!(
        enabled = back_stack.with(|stack| stack.len() > 1),
        on_back = move || {
            back_stack.edit(|stack| stack.pop());
        }
    );
    let current = back_stack.with(|stack| *stack.last().expect("the stack has its root"));
    match current {
        Screen::Inbox => Inbox!(
            on_open = move |id: u32| navigate(Screen::Message(id)),
            on_settings = move || navigate(Screen::Settings)
        ),
        // A key per message: each message's screen gets its own remembered state.
        Screen::Message(id) => key(id, || MessageScreen!(id)),
        Screen::Settings => SettingsScreen!(),
    }
}
// ANCHOR_END: app

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
                    ListItem!(
                        headline_content = { Text!("Message {}", id) },
                        modifier = Modifier.clickable(move || on_open(id))
                    );
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
