//! A tour of the toolkit: a scaffold with a top app bar (its menu opening a navigation drawer,
//! its search filtering the cards; it slides away as the cards scroll and comes back as they
//! scroll back) and an extended floating action button adding a card (and saying so in a
//! snackbar; just its icon once the cards are scrolled), Material's selection controls, text
//! fields, progress indicators, an animated panel and a lazy list of cards with a scrollbar.
//! A card opens a screen of its own (a `NavHost`): back returns to the tour as it was left.

#![allow(non_snake_case)]

use compose::animation::*;
use compose::foundation::layout::*;
use compose::foundation::{
    LazyColumn, LazyListScope, LazyListState, TooltipArea, VerticalScrollbar,
    is_system_in_dark_theme, remember_lazy_list_scrollbar_adapter, remember_lazy_list_state,
};
use compose::material3::*;
use compose::navigation::*;
use compose::runtime::*;
use compose::ui::*;

#[composable]
pub fn Gallery() {
    GalleryWith(false);
}

/// The gallery's screens: the tour, and a card's own.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Destination {
    Home,
    Card(u32),
}

/// Saved as a number, so the back stack survives the activity's recreation.
impl Saveable for Destination {
    fn save(&self) -> SavedValue {
        SavedValue::Int(match *self {
            Destination::Home => -1,
            Destination::Card(card) => i64::from(card),
        })
    }

    fn restore(saved: &SavedValue) -> Option<Destination> {
        match *saved {
            SavedValue::Int(-1) => Some(Destination::Home),
            SavedValue::Int(card) => u32::try_from(card).ok().map(Destination::Card),
            _ => None,
        }
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
        let nav_controller = remember_nav_controller(Destination::Home);
        NavHost!(nav_controller, move |destination: Destination| match destination {
            Destination::Home => Home!(
                about,
                on_open_card = move |card: u32| nav_controller.navigate(Destination::Card(card))
            ),
            Destination::Card(card) => CardScreen!(
                card,
                on_back = move || {
                    nav_controller.pop_back_stack();
                }
            ),
        });
    });
}

/// The tour: a drawer, a scaffold with its bars, the controls and the cards.
#[composable]
fn Home(about: bool, on_open_card: impl Fn(u32) + 'static) {
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
            ModalDrawerSheet!({
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
    TopAppBar!(
        {
            if searching.get() {
                TextField!(
                    query.get(),
                    move |value| query.set(value),
                    modifier = Modifier.fill_max_width().focus_requester(search_field),
                    text_style = MaterialTheme::typography().body_large.clone(),
                    placeholder = Some(|| Text!("Search the cards")),
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
    Slider!(level.get(), move |value| level.set(value));
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
