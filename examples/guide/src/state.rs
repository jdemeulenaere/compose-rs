//! State (docs/guide/kotlin-to-rust.md, "State").

use compose::foundation::layout::*;
use compose::foundation::{LazyColumn, LazyListScope, remember_lazy_list_state};
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

// ANCHOR: counter
#[composable]
pub fn Counter() {
    // The slot keeps the state; `count` is a Copy handle, captured by `move` closures.
    let count = remember(|| mutable_state_of(0));
    Button!(on_click = move || count.update(|c| c + 1), {
        Text!("Clicked {} times", count.get());
    });
}
// ANCHOR_END: counter

// ANCHOR: saveable
#[composable]
pub fn Draft() {
    // Saved with the Android view's instance state: kept across the activity's recreation.
    let text = remember_saveable(|| mutable_state_of(String::new()));
    TextField!(text.get(), move |value| text.set(value), modifier = Modifier.test_tag("draft"));
}
// ANCHOR_END: saveable

// ANCHOR: derived
#[composable]
pub fn LongList() {
    let list = remember_lazy_list_state!();
    // Recomposes its readers when the answer changes, not on every scrolled pixel.
    let scrolled = remember(move || derived_state_of(move || list.first_visible_item_index() > 0));
    Box!(modifier = Modifier.fill_max_size(), {
        LazyColumn!(modifier = Modifier.fill_max_size(), state = list, |scope: LazyListScope| {
            scope.items(1_000, |index| Text!("Item {}", index));
        });
        if scrolled.get() {
            SmallFloatingActionButton!(on_click = move || list.scroll_to_item(0, 0), {
                Icon!(icons::filled::keyboard_arrow_up(), Some("Back to the top"));
            });
        }
    });
}
// ANCHOR_END: derived

// ANCHOR: hoisting
/// Stateless: the value comes in, changes go out.
#[composable]
pub fn Stepper(value: i32, on_value_change: impl Fn(i32) + 'static) {
    Row!(vertical_alignment = Alignment::CENTER_VERTICALLY, {
        TextButton!(on_click = move || on_value_change(value - 1), {
            Text!("-");
        });
        Text!("{}", value);
        TextButton!(on_click = move || on_value_change(value + 1), {
            Text!("+");
        });
    });
}

/// Stateful: owns the state, hoisted out of `Stepper`.
#[composable]
pub fn Quantity() {
    let quantity = remember_saveable(|| mutable_state_of(1));
    Stepper!(quantity.get(), move |value: i32| quantity.set(value.max(0)));
}
// ANCHOR_END: hoisting

// ANCHOR: holders
#[composable]
pub fn Menu() {
    let drawer = remember_drawer_state(DrawerValue::Closed); // a Copy handle, as state
    let scope = remember_coroutine_scope(); // Copy too
    ModalNavigationDrawer!(
        {
            ModalDrawerSheet!({
                Text!("Settings", modifier = Modifier.padding(16.dp()));
            });
        },
        drawer_state = drawer,
        {
            Button!(
                on_click = move || {
                    scope.launch(async move { drawer.open().await });
                },
                {
                    Text!("Open the menu");
                }
            );
        }
    );
}
// ANCHOR_END: holders

// ANCHOR: clone_on_use
#[composable]
pub fn Profile(name: SharedString, on_open: impl Fn(SharedString) + 'static) {
    Card!({
        Text!(name); // in a lambda, a capture given by itself is cloned
        TextButton!(on_click = move || on_open(name), {
            Text!("Open");
        });
    });
    // Top-level code is plain Rust: this moves `name`, so it's its last use.
    Text!(name);
}
// ANCHOR_END: clone_on_use

// ANCHOR: model
/// A contact: `#[derive(ComposeArg)]` compares field by field (numbers and short strings by
/// value, `Rc`s and state handles by identity).
#[derive(Clone, PartialEq, ComposeArg)]
pub struct Contact {
    pub id: u64,
    pub name: SharedString,
    /// A short list, compared by value (`PartialEq`).
    #[compose_arg(eq)]
    pub tags: Vec<String>,
}

/// Skips when given the same list (by identity): keep the list in state, don't rebuild it.
#[composable]
pub fn Contacts(contacts: ImmutableList<Contact>) {
    Column!({
        for contact in contacts.iter() {
            key(contact.id, || ContactRow(contact.clone()));
        }
    });
}

/// Skips when its contact is the same.
#[composable]
pub fn ContactRow(contact: Contact) {
    Text!("{} ({})", contact.name, contact.tags.join(", "));
}

#[composable]
pub fn ContactsScreen() {
    let contacts = remember(|| {
        mutable_state_of(ImmutableList::from(vec![
            Contact { id: 1, name: "Ada".into(), tags: vec!["math".into()] },
            Contact { id: 2, name: "Grace".into(), tags: vec!["navy".into(), "cobol".into()] },
        ]))
    });
    Contacts!(contacts.get());
}
// ANCHOR_END: model
