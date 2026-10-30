//! Navigation with a NavDisplay (docs/guide/kotlin-to-rust.md, "Navigation").

use compose::foundation::layout::*;
use compose::foundation::{LazyColumn, LazyListScope};
use compose::material3::*;
use compose::navigation::*;
use compose::runtime::*;

// ANCHOR: screens
/// The app's screens: Navigation 3's keys, as an enum. Saved field by field, so the back stack
/// survives the activity's recreation.
#[derive(Clone, Copy, PartialEq, Debug, ComposeArg, Saveable)]
pub enum Screen {
    Inbox,
    Message(u32),
    Settings,
}
// ANCHOR_END: screens

// ANCHOR: app
#[composable]
pub fn App() {
    // The back stack, starting at the inbox: saved, so it survives the activity's recreation.
    let back_stack = remember_nav_back_stack([Screen::Inbox]);
    // Shows the last entry; back pops the stack, and leaves the app at the inbox.
    NavDisplay!(back_stack, entry_provider = move |screen: Screen| entry(back_stack, screen));
}

/// What each screen shows: any `Fn(T) -> NavEntry<T>`.
fn entry(back_stack: NavBackStack<Screen>, screen: Screen) -> NavEntry<Screen> {
    match screen {
        Screen::Inbox => NavEntry::new(screen, move |_| {
            Inbox!(
                on_open = move |id: u32| back_stack.push(Screen::Message(id)),
                on_settings = move || back_stack.push(Screen::Settings)
            );
        }),
        Screen::Message(id) => NavEntry::new(screen, move |_| MessageScreen!(id)),
        Screen::Settings => NavEntry::new(screen, |_| SettingsScreen!()),
    }
}
// ANCHOR_END: app

// ANCHOR: redirect
/// Sends the user to `to` as soon as it shows, in place of itself: the stack is the app's, so it's
/// edited as any list. It's state, so it goes in an effect or an event handler.
#[composable]
pub fn Redirect(back_stack: NavBackStack<Screen>, to: Screen) {
    LaunchedEffect((), async move {
        back_stack.clear();
        back_stack.push(Screen::Inbox);
        back_stack.push(to);
    });
}
// ANCHOR_END: redirect

// ANCHOR: deep_link
/// What a link to a message holds: decoded from the URI's arguments, by name.
#[derive(DeepLinkKey)]
pub struct MessageLink {
    pub id: u32,
}

/// The back stack a link opens, if it's one of the app's: the message, over the inbox.
pub fn back_stack_of_link(uri: &str) -> Option<Vec<Screen>> {
    let matcher =
        UriDeepLinkMatcher::<MessageLink>::new(DeepLinkUri::new("example.com/messages/{id}"))
            .map_key(|link| Screen::Message(link.id))
            .with_back_stack(|result| vec![Screen::Inbox, *result.key()]);
    let result = matcher.try_match(&DeepLinkRequest::of(uri))?;
    Some(result.back_stack().to_vec())
}
// ANCHOR_END: deep_link

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
