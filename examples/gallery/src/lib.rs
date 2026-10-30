//! A tour of the toolkit: a scaffold with a top app bar (its menu opening a navigation drawer,
//! its search filtering the cards; it slides away as the cards scroll and comes back as they
//! scroll back) and an extended floating action button adding a card (and saying so in a
//! snackbar; just its icon once the cards are scrolled), Material's selection controls, text
//! fields, progress indicators, an animated panel and a lazy list of cards with a scrollbar.
//! A card opens a screen of its own (a `NavDisplay`): back returns to the tour as it was left.
//! The drawer opens "More controls": tabs of what the tour has no room for, to try on a phone with
//! a keyboard and a screen reader.

#![allow(non_snake_case)]

use compose::animation::*;
use compose::foundation::layout::*;
use compose::foundation::{
    LazyColumn, LazyListScope, LazyListState, PasswordVisualTransformation, ScrollModifiers,
    TooltipArea, VerticalScrollbar, is_system_in_dark_theme, remember_lazy_list_scrollbar_adapter,
    remember_lazy_list_state, remember_scroll_state,
};
use compose::material3::*;
use compose::navigation::*;
use compose::runtime::*;
use compose::ui::*;

#[composable]
pub fn Gallery() {
    GalleryWith(false);
}

/// The gallery's screens: the tour, and a card's own. Saved, so the back stack survives the
/// activity's recreation.
#[derive(Clone, Copy, PartialEq, Debug, Saveable)]
pub enum Destination {
    Home,
    Card(u32),
    MoreControls,
}

/// What each screen shows.
fn destination_entry(
    back_stack: NavBackStack<Destination>,
    about: bool,
    destination: Destination,
) -> NavEntry<Destination> {
    match destination {
        Destination::Home => NavEntry::new(destination, move |_| {
            Home!(
                about,
                on_open_card = move |card: u32| back_stack.push(Destination::Card(card)),
                on_open_more = move || back_stack.push(Destination::MoreControls)
            );
        }),
        Destination::Card(card) => NavEntry::new(destination, move |_| {
            CardScreen!(
                card,
                on_back = move || {
                    back_stack.remove_last_or_none();
                }
            );
        }),
        Destination::MoreControls => NavEntry::new(destination, move |_| {
            MoreControlsScreen!(
                on_back = move || {
                    back_stack.remove_last_or_none();
                }
            );
        }),
    }
}

/// The gallery, with its "About" dialog open from the start if `about`.
#[composable]
pub fn GalleryWith(about: bool) {
    // The dark scheme when the system's dark theme is on, as Compose apps pick theirs.
    let color_scheme =
        if is_system_in_dark_theme() { dark_color_scheme() } else { light_color_scheme() };
    MaterialTheme!(color_scheme = color_scheme, {
        // The tour, and above it the card opened, if any. The tour's saveable state (the
        // cards, the search, the list's position, the controls) is kept while a card shows.
        let back_stack = remember_nav_back_stack([Destination::Home]);
        NavDisplay!(
            back_stack,
            entry_provider = move |destination: Destination| {
                destination_entry(back_stack, about, destination)
            }
        );
    });
}

/// The tour: a drawer, a scaffold with its bars, the controls and the cards.
#[composable]
fn Home(about: bool, on_open_card: impl Fn(u32) + 'static, on_open_more: impl Fn() + 'static) {
    let cards = remember_saveable(|| mutable_state_of(FIRST_CARDS));
    let menu = remember(|| mutable_state_of(false));
    let dialog = remember_saveable(|| mutable_state_of(about));
    let searching = remember_saveable(|| mutable_state_of(false));
    let query = remember_saveable(|| mutable_state_of(String::new()));
    let drawer = remember_drawer_state!(DrawerValue::Closed);
    let list = remember_lazy_list_state!();
    let scroll_behavior = TopAppBarDefaults::enter_always_scroll_behavior();
    let snackbars = remember(SnackbarHostState::new);
    let scope = remember_coroutine_scope();
    // Closes the drawer.
    let close_drawer = callback(move || {
        scope.launch(async move { drawer.close().await });
    });
    ModalNavigationDrawer!(
        {
            ModalDrawerSheet!(drawer_state = Some(drawer), {
                Text!(
                    "compose-rs",
                    modifier = Modifier.padding((28.dp(), 24.dp(), 16.dp(), 16.dp())),
                    style = MaterialTheme::typography().title_medium.clone()
                );
                NavigationDrawerItem!(
                    { Text!("Gallery") },
                    true,
                    close_drawer,
                    modifier = NavigationDrawerItemDefaults::item_padding(Modifier),
                    icon = Some(|| Icon!(icons::filled::home(), None))
                );
                NavigationDrawerItem!(
                    { Text!("Reset the cards") },
                    false,
                    move || {
                        cards.set(FIRST_CARDS);
                        query.set(String::new());
                        close_drawer();
                    },
                    modifier = NavigationDrawerItemDefaults::item_padding(Modifier),
                    icon = Some(|| Icon!(icons::filled::delete(), None))
                );
                NavigationDrawerItem!(
                    { Text!("About") },
                    false,
                    move || {
                        dialog.set(true);
                        close_drawer();
                    },
                    modifier = NavigationDrawerItemDefaults::item_padding(Modifier),
                    icon = Some(|| Icon!(icons::filled::info(), None))
                );
                NavigationDrawerItem!(
                    { Text!("More controls") },
                    false,
                    move || {
                        on_open_more();
                        close_drawer();
                    },
                    modifier = NavigationDrawerItemDefaults::item_padding(Modifier),
                    icon = Some(|| Icon!(icons::filled::more_vert(), None))
                );
            });
        },
        drawer_state = drawer,
        {
            Scaffold!(
                modifier = Modifier.nested_scroll(scroll_behavior.nested_scroll_connection()),
                top_bar = {
                    TopBar(drawer, scroll_behavior, searching, query, menu, cards, dialog);
                },
                snackbar_host = {
                    SnackbarHost!(snackbars);
                },
                floating_action_button = {
                    ExtendedFloatingActionButton!(
                        text = { Text!("Add a card") },
                        icon = { Icon!(icons::filled::add(), Some("Add a card")) },
                        expanded = list.first_visible_item_index() == 0,
                        on_click = move || {
                            let added = cards.get();
                            cards.set(added + 1);
                            scope.launch(async move {
                                snackbars
                                    .show_snackbar(format!("Card {added} added at the top"))
                                    .await;
                            });
                        },
                    );
                },
                |padding: PaddingValues| {
                    if dialog.get() {
                        AboutDialog(dialog);
                    }
                    Column!(
                        modifier = Modifier.padding(padding).padding(16.dp()).fill_max_size(),
                        vertical_arrangement = Arrangement::spaced_by(12.dp()),
                        |column| {
                            Controls(cards);
                            Cards(
                                cards.get(),
                                query.get(),
                                list,
                                Modifier.weight(column, 1.0),
                                on_open_card,
                            );
                        }
                    );
                }
            );
        }
    );
}

/// How many cards the gallery starts with.
const FIRST_CARDS: u32 = 20;

/// The top bar: the menu opens the drawer; search turns the title into a field filtering the
/// cards; "More" offers Reset and About.
#[composable]
fn TopBar(
    drawer: DrawerState,
    scroll_behavior: TopAppBarScrollBehavior,
    searching: MutableState<bool>,
    query: MutableState<String>,
    menu: MutableState<bool>,
    cards: MutableState<u32>,
    dialog: MutableState<bool>,
) {
    let scope = remember_coroutine_scope();
    let search_field = remember(FocusRequester::new);
    let keyboard = LOCAL_SOFTWARE_KEYBOARD_CONTROLLER.current();
    TopAppBar!(
        {
            if searching.get() {
                TextField!(
                    query.get(),
                    move |value| query.set(value),
                    modifier = Modifier.fill_max_width().focus_requester(search_field),
                    text_style = MaterialTheme::typography().body_large.clone(),
                    placeholder = Some(|| Text!("Search the cards")),
                    keyboard_options = KeyboardOptions {
                        ime_action: ImeAction::Search,
                        ..KeyboardOptions::DEFAULT
                    },
                    // The cards filter as it's typed: the key just shows them.
                    keyboard_actions = KeyboardActions::new().on_search(move || {
                        if let Some(keyboard) = &keyboard {
                            keyboard.hide();
                        }
                    }),
                    single_line = true,
                );
            } else {
                Text!("Gallery");
            }
        },
        navigation_icon = {
            IconButton!(
                on_click = move || {
                    scope.launch(async move { drawer.open().await });
                },
                {
                    Icon!(icons::filled::menu(), Some("Menu"));
                }
            );
        },
        scroll_behavior = Some(scroll_behavior),
        actions = move |_row: RowScope| {
            TooltipArea!(
                move || PlainTooltip!({
                    Text!(if searching.get() { "Close the search" } else { "Search the cards" })
                }),
                {
                    IconButton!(
                        on_click = move || {
                            if searching.get() {
                                query.set(String::new());
                            }
                            searching.set(!searching.get());
                        },
                        {
                            if searching.get() {
                                Icon!(icons::filled::close(), Some("Close the search"));
                            } else {
                                Icon!(icons::filled::search(), Some("Search"));
                            }
                        }
                    );
                }
            );
            Box!({
                IconButton!(on_click = move || menu.set(true), {
                    Icon!(icons::filled::more_vert(), Some("More"));
                });
                DropdownMenu!(menu.get(), move || menu.set(false), {
                    DropdownMenuItem!(
                        {
                            Text!("Reset the cards");
                        },
                        move || {
                            cards.set(FIRST_CARDS);
                            query.set(String::new());
                            menu.set(false);
                        },
                        leading_icon = {
                            Icon!(icons::filled::delete(), None);
                        }
                    );
                    DropdownMenuItem!(
                        {
                            Text!("About");
                        },
                        move || {
                            menu.set(false);
                            dialog.set(true);
                        },
                        leading_icon = {
                            Icon!(icons::filled::info(), None);
                        }
                    );
                });
            });
        }
    );
    // The field takes the focus as it shows (the keyboard opens).
    LaunchedEffect(searching.get(), async move {
        if searching.get() {
            search_field.request_focus();
        }
    });
}

#[composable]
fn AboutDialog(dialog: MutableState<bool>) {
    AlertDialog!(
        on_dismiss_request = move || dialog.set(false),
        confirm_button = {
            TextButton!(on_click = move || dialog.set(false), {
                Text!("OK");
            });
        },
        icon = {
            Icon!(icons::filled::info(), None);
        },
        title = {
            Text!("compose-rs");
        },
        text = {
            Text!(
                "Jetpack Compose's model, in Rust: drawn with Skia on desktop, and with the system's Canvas on Android."
            );
        },
    );
}

/// The cards, newest first, those matching the search, with a scrollbar as on desktop.
#[composable]
fn Cards(
    count: u32,
    query: String,
    list: LazyListState,
    modifier: Modifier,
    on_open: impl Fn(u32) + 'static,
) {
    let query = query.trim().to_lowercase();
    let shown: std::rc::Rc<Vec<u32>> = std::rc::Rc::new(
        (0..count).rev().filter(|index| format!("card {index}").contains(&query)).collect(),
    );
    if shown.is_empty() {
        Text!("No card matches “{}”", query, modifier = modifier.padding(16.dp()));
        return;
    }
    Row!(modifier = modifier.fill_max_width(), |row: RowScope| {
        LazyColumn!(
            modifier = Modifier.weight(row, 1.0),
            state = list,
            vertical_arrangement = Arrangement::spaced_by(8.dp()),
            move |scope: LazyListScope| {
                scope.items_keyed(
                    shown.len(),
                    move |index| u64::from(shown[index]),
                    move |index| {
                        let card = shown[index];
                        Card!(
                            on_click = move || on_open(card),
                            modifier = Modifier.fill_max_width(),
                            {
                                Row!(
                                    modifier = Modifier.padding(12.dp()),
                                    horizontal_arrangement = Arrangement::spaced_by(8.dp()),
                                    vertical_alignment = Alignment::CENTER_VERTICALLY,
                                    {
                                        Icon!(icons::filled::star(), None);
                                        Text!("Card {}", card);
                                    }
                                );
                            }
                        );
                    },
                );
            }
        );
        let adapter = remember_lazy_list_scrollbar_adapter(list);
        VerticalScrollbar!(
            adapter,
            modifier = Modifier.fill_max_height().padding((4.dp(), 0.dp(), 0.dp(), 0.dp()))
        );
    });
}

#[composable]
fn Controls(cards: MutableState<u32>) {
    // Saveable: kept while a card's screen shows, and across the activity's recreation.
    let switched = remember_saveable(|| mutable_state_of(true));
    let choice = remember_saveable(|| mutable_state_of(0));
    let level = remember_saveable(|| mutable_state_of(0.4f32));
    Row!(vertical_alignment = Alignment::CENTER_VERTICALLY, {
        Switch!(switched.get(), Some(move |checked| switched.set(checked)));
        for index in 0..3 {
            RadioButton!(choice.get() == index, Some(move || choice.set(index)));
        }
        Text!("Cards: {}", cards.get());
    });
    let name = remember_saveable(|| mutable_state_of(String::new()));
    let city = remember_saveable(|| mutable_state_of(String::from("Paris")));
    Row!(horizontal_arrangement = Arrangement::spaced_by(8.dp()), |row: RowScope| {
        TextField!(
            name.get(),
            move |value| name.set(value),
            modifier = Modifier.weight(row, 1.0),
            label = Some(|| Text!("Name")),
            single_line = true,
        );
        OutlinedTextField!(
            city.get(),
            move |value| city.set(value),
            modifier = Modifier.weight(row, 1.0),
            label = Some(|| Text!("City")),
            single_line = true,
        );
    });
    let slider = remember_slider_state!(level.get());
    Slider!(slider, move |value| {
        slider.set_value(value);
        level.set(value);
    });
    Row!(
        modifier = Modifier.padding((0.dp(), 8.dp())),
        horizontal_arrangement = Arrangement::spaced_by(16.dp()),
        vertical_alignment = Alignment::CENTER_VERTICALLY,
        |row: RowScope| {
            CircularProgressIndicator!(move || level.get());
            LinearProgressIndicator!(move || level.get(), modifier = Modifier.weight(row, 1.0));
            if switched.get() {
                IndeterminateCircularProgressIndicator!();
            }
        }
    );
    AnimatedVisibility!(switched.get(), {
        Card!(modifier = Modifier.fill_max_width(), {
            Text!("Shown while the switch is on", modifier = Modifier.padding(12.dp()));
        });
    });
    HorizontalDivider!();
}

/// A card's own screen: a top bar whose arrow goes back, and a saved "like".
#[composable]
fn CardScreen(card: u32, on_back: impl Fn() + 'static) {
    let liked = remember_saveable(|| mutable_state_of(false));
    Scaffold!(
        top_bar = {
            TopAppBar!(
                {
                    Text!("Card {}", card);
                },
                navigation_icon = {
                    IconButton!(on_click = move || on_back(), {
                        Icon!(icons::filled::arrow_back(), Some("Back"));
                    });
                }
            );
        },
        |padding: PaddingValues| {
            Column!(
                modifier = Modifier.padding(padding).padding(16.dp()).fill_max_size(),
                vertical_arrangement = Arrangement::spaced_by(12.dp()),
                {
                    Text!(
                        "A screen of its own: back, or the arrow, returns to the gallery as it was.",
                        style = MaterialTheme::typography().body_large.clone()
                    );
                    Row!(vertical_alignment = Alignment::CENTER_VERTICALLY, {
                        IconToggleButton!(liked.get(), move |checked| liked.set(checked), {
                            if liked.get() {
                                Icon!(icons::filled::favorite(), Some("Like"));
                            } else {
                                Icon!(icons::filled::favorite_border(), Some("Like"));
                            }
                        });
                        Text!(if liked.get() { "Liked" } else { "Not liked yet" });
                    });
                }
            );
        }
    );
}

/// What the tour has no room for, a tab for each kind.
#[composable]
fn MoreControlsScreen(on_back: impl Fn() + 'static) {
    let tab = remember_saveable(|| mutable_state_of(0));
    let scroll = remember_scroll_state!();
    Scaffold!(
        top_bar = {
            TopAppBar!(
                {
                    Text!("More controls");
                },
                navigation_icon = {
                    IconButton!(on_click = move || on_back(), {
                        Icon!(icons::filled::arrow_back(), Some("Back"));
                    });
                }
            );
        },
        |padding: PaddingValues| {
            Column!(modifier = Modifier.padding(padding).fill_max_size(), |column| {
                PrimaryTabRow!(tab.get(), {
                    for (index, title) in ["Fields", "Choices", "Windows"].into_iter().enumerate() {
                        Tab!(
                            tab.get() == index,
                            move || tab.set(index),
                            text = Some(move || Text!(title))
                        );
                    }
                });
                Column!(
                    modifier = Modifier
                        .weight(column, 1.0)
                        .fill_max_width()
                        .vertical_scroll(scroll)
                        .padding(16.dp()),
                    vertical_arrangement = Arrangement::spaced_by(12.dp()),
                    {
                        match tab.get() {
                            0 => FieldsTab(),
                            1 => ChoicesTab(),
                            _ => WindowsTab(),
                        }
                    }
                );
            });
        }
    );
}

/// A field of each kind the keyboard treats differently, and a way out of them all.
#[composable]
fn FieldsTab() {
    // Not saveable: a password isn't kept in the saved state.
    let password = remember(|| mutable_state_of(String::new()));
    let shown = remember(|| mutable_state_of(false));
    let number = remember_saveable(|| mutable_state_of(String::new()));
    let phone = remember_saveable(|| mutable_state_of(String::new()));
    let notes = remember_saveable(|| mutable_state_of(String::new()));
    let focus = LOCAL_FOCUS_MANAGER.current();
    let options = |keyboard_type, ime_action| KeyboardOptions {
        keyboard_type,
        ime_action,
        ..KeyboardOptions::DEFAULT
    };
    TextField!(
        password.get(),
        move |value| password.set(value),
        modifier = Modifier.fill_max_width(),
        label = Some(|| Text!("Password")),
        trailing_icon = Some(move || {
            IconButton!(on_click = move || shown.set(!shown.get()), {
                let description =
                    if shown.get() { "Hide the password" } else { "Show the password" };
                Icon!(icons::filled::lock(), Some(description));
            });
        }),
        visual_transformation = if shown.get() {
            VisualTransformation::NONE
        } else {
            PasswordVisualTransformation::new().into()
        },
        keyboard_options = options(KeyboardType::Password, ImeAction::Next),
        single_line = true,
    );
    TextField!(
        number.get(),
        move |value| number.set(value),
        modifier = Modifier.fill_max_width(),
        label = Some(|| Text!("Number")),
        keyboard_options = options(KeyboardType::Number, ImeAction::Next),
        single_line = true,
    );
    TextField!(
        phone.get(),
        move |value| phone.set(value),
        modifier = Modifier.fill_max_width(),
        label = Some(|| Text!("Phone")),
        keyboard_options = options(KeyboardType::Phone, ImeAction::Done),
        single_line = true,
    );
    TextField!(
        notes.get(),
        move |value| notes.set(value),
        modifier = Modifier.fill_max_width(),
        label = Some(|| Text!("Notes")),
        min_lines = 3,
    );
    Button!(on_click = move || focus.clear_focus(false), {
        Text!("Clear the focus");
    });
}

/// A checkbox of three states over its two children, rows selected with no role, messages with
/// actions of their own, and a group read in another order than it's shown.
#[composable]
fn ChoicesTab() {
    let milk = remember_saveable(|| mutable_state_of(false));
    let eggs = remember_saveable(|| mutable_state_of(false));
    let both = match (milk.get(), eggs.get()) {
        (true, true) => ToggleableState::On,
        (false, false) => ToggleableState::Off,
        _ => ToggleableState::Indeterminate,
    };
    Row!(vertical_alignment = Alignment::CENTER_VERTICALLY, {
        TriStateCheckbox!(
            both,
            Some(move || {
                let next = both != ToggleableState::On;
                milk.set(next);
                eggs.set(next);
            }),
            modifier = Modifier.semantics(|config| config.set_content_description("Milk and eggs"))
        );
        Text!("Milk and eggs");
    });
    for (title, item) in [("Milk", milk), ("Eggs", eggs)] {
        Row!(
            modifier = Modifier.padding((24.dp(), 0.dp(), 0.dp(), 0.dp())),
            vertical_alignment = Alignment::CENTER_VERTICALLY,
            {
                Checkbox!(
                    checked = item.get(),
                    on_checked_change = move |checked| item.set(checked),
                    modifier = Modifier.semantics(|config| config.set_content_description(title))
                );
                Text!(title);
            }
        );
    }
    HorizontalDivider!();

    let size = remember_saveable(|| mutable_state_of(0));
    Column!(modifier = Modifier.selectable_group(), {
        for (index, title) in ["Small", "Large"].into_iter().enumerate() {
            let selected = size.get() == index;
            Row!(
                modifier = Modifier
                    .fill_max_width()
                    .selectable(selected, move || size.set(index))
                    .padding(12.dp()),
                horizontal_arrangement = Arrangement::spaced_by(8.dp()),
                vertical_alignment = Alignment::CENTER_VERTICALLY,
                {
                    Box!(modifier = Modifier.size(24.dp()), {
                        if selected {
                            Icon!(icons::filled::check(), None);
                        }
                    });
                    Text!(title);
                }
            );
        }
    });
    HorizontalDivider!();

    // Each message's number and whether it's starred, in the list's order.
    let messages = remember(|| mutable_state_of(vec![(1, false), (2, false), (3, false)]));
    for (message, starred) in messages.get() {
        let star = move || {
            let mut all = messages.get();
            if let Some(entry) = all.iter_mut().find(|entry| entry.0 == message) {
                entry.1 = !entry.1;
            }
            messages.set(all);
            true
        };
        let move_to_the_end = move || {
            let mut all = messages.get();
            if let Some(at) = all.iter().position(|entry| entry.0 == message) {
                let entry = all.remove(at);
                all.push(entry);
            }
            messages.set(all);
            true
        };
        let label = if starred { "Unstar" } else { "Star" };
        Card!(
            modifier = Modifier.fill_max_width().semantics_with(true, move |config| {
                config.set_custom_actions(vec![
                    CustomAccessibilityAction::new(label, star),
                    CustomAccessibilityAction::new("Move to the end", move_to_the_end),
                ]);
            }),
            {
                let suffix = if starred { ", starred" } else { "" };
                Text!("Message {}{}", message, suffix, modifier = Modifier.padding(12.dp()));
            }
        );
    }
    HorizontalDivider!();

    Row!(
        modifier = Modifier.semantics(|config| config.set_is_traversal_group(true)),
        horizontal_arrangement = Arrangement::spaced_by(12.dp()),
        {
            for (text, index) in [("Read second", 2.0), ("Read third", 3.0), ("Read first", 1.0)] {
                Text!(
                    text,
                    modifier = Modifier.semantics(|config| config.set_traversal_index(index))
                );
            }
        }
    );
    Text!("Read after the group");
}

/// A field, and what can open over it: a bottom sheet, and a dialog with a field of its own.
#[composable]
fn WindowsTab() {
    let behind = remember_saveable(|| mutable_state_of(String::new()));
    let sheet = remember_saveable(|| mutable_state_of(false));
    let dialog = remember_saveable(|| mutable_state_of(false));
    TextField!(
        behind.get(),
        move |value| behind.set(value),
        modifier = Modifier.fill_max_width(),
        label = Some(|| Text!("Behind")),
        single_line = true,
    );
    Button!(on_click = move || sheet.set(true), {
        Text!("Open a bottom sheet");
    });
    Button!(on_click = move || dialog.set(true), {
        Text!("Open a dialog with a field");
    });
    if sheet.get() {
        ModalBottomSheet!(move || sheet.set(false), |_| {
            // Tall enough to rest half way, with the handle to expand it from there.
            for line in 1..=12 {
                Text!("Line {}", line, modifier = Modifier.padding(16.dp()));
            }
        });
    }
    if dialog.get() {
        FieldDialog(dialog);
    }
}

/// A dialog with no title, whose field takes the focus as it opens.
#[composable]
fn FieldDialog(dialog: MutableState<bool>) {
    let value = remember_saveable(|| mutable_state_of(String::new()));
    let requester = remember(FocusRequester::new);
    AlertDialog!(
        on_dismiss_request = move || dialog.set(false),
        confirm_button = {
            TextButton!(on_click = move || dialog.set(false), {
                Text!("OK");
            });
        },
        text = {
            TextField!(
                value.get(),
                move |new| value.set(new),
                modifier = Modifier.focus_requester(requester),
                label = Some(|| Text!("In the dialog")),
                single_line = true,
            );
            LaunchedEffect((), async move {
                requester.request_focus();
            });
        },
    );
}
