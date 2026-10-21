//! A tour of the toolkit: a scaffold with a top app bar and a floating action button, Material's
//! selection controls, cards, icons, an animated panel and a lazy list.

#![allow(non_snake_case)]

use compose::animation::*;
use compose::foundation::layout::*;
use compose::foundation::{
    LazyColumn, LazyListScope, TooltipArea, VerticalScrollbar,
    remember_lazy_list_scrollbar_adapter, remember_lazy_list_state,
};
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

#[composable]
pub fn Gallery() {
    GalleryWith(false);
}

/// The gallery, with its "About" dialog open from the start if `about`.
#[composable]
pub fn GalleryWith(about: bool) {
    MaterialTheme!({
        let count = remember(|| mutable_state_of(0));
        let menu = remember(|| mutable_state_of(false));
        let dialog = remember(|| mutable_state_of(about));
        Scaffold!(
            top_bar = {
                TopAppBar!(
                    {
                        Text!("Gallery");
                    },
                    navigation_icon = {
                        IconButton!(on_click = || {}, {
                            Icon!(icons::filled::menu(), Some("Menu"));
                        });
                    },
                    actions = |_row: RowScope| {
                        TooltipArea!(|| PlainTooltip!({ Text!("Search the gallery") }), {
                            IconButton!(on_click = || {}, {
                                Icon!(icons::filled::search(), Some("Search"));
                            });
                        });
                        Box!({
                            IconButton!(on_click = move || menu.set(true), {
                                Icon!(icons::filled::more_vert(), Some("More"));
                            });
                            DropdownMenu!(menu.get(), move || menu.set(false), {
                                DropdownMenuItem!(
                                    {
                                        Text!("Reset");
                                    },
                                    move || {
                                        count.set(0);
                                        menu.set(false);
                                    },
                                    leading_icon = {
                                        Icon!(icons::filled::close(), None);
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
            },
            floating_action_button = {
                FloatingActionButton!(on_click = move || count.update(|count| count + 1), {
                    Icon!(icons::filled::add(), Some("Add"));
                });
            },
            |padding: PaddingValues| {
                if dialog.get() {
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
                                "Jetpack Compose's model, in Rust: this gallery is drawn with Skia."
                            );
                        },
                    );
                }
                Column!(
                    modifier = Modifier.padding_values(padding).padding(16.dp()).fill_max_size(),
                    vertical_arrangement = Arrangement::spaced_by(12.dp()),
                    |column| {
                        Controls(count);
                        // The list with its scrollbar, as on desktop.
                        let list = remember_lazy_list_state(0, 0);
                        Row!(
                            modifier = Modifier.weight(column, 1.0).fill_max_width(),
                            |row: RowScope| {
                                LazyColumn!(
                                    modifier = Modifier.weight(row, 1.0),
                                    state = list.clone(),
                                    vertical_arrangement = Arrangement::spaced_by(8.dp()),
                                    |scope: LazyListScope| {
                                        scope.items(100, |index| {
                                            Card!(modifier = Modifier.fill_max_width(), {
                                                Row!(
                                                    modifier = Modifier.padding(12.dp()),
                                                    horizontal_arrangement =
                                                        Arrangement::spaced_by(8.dp()),
                                                    vertical_alignment =
                                                        Alignment::CENTER_VERTICALLY,
                                                    {
                                                        Icon!(icons::filled::star(), None);
                                                        Text!("Card {}", index);
                                                    }
                                                );
                                            });
                                        });
                                    }
                                );
                                let adapter = remember_lazy_list_scrollbar_adapter(list.clone());
                                VerticalScrollbar!(
                                    adapter,
                                    modifier = Modifier.fill_max_height().padding_sides(
                                        4.dp(),
                                        0.dp(),
                                        0.dp(),
                                        0.dp()
                                    )
                                );
                            }
                        );
                    }
                );
            }
        );
    });
}

#[composable]
fn Controls(count: MutableState<i32>) {
    let switched = remember(|| mutable_state_of(true));
    let choice = remember(|| mutable_state_of(0));
    let level = remember(|| mutable_state_of(0.4f32));
    Row!(vertical_alignment = Alignment::CENTER_VERTICALLY, {
        Switch!(switched.get(), Some(move |checked| switched.set(checked)));
        for index in 0..3 {
            RadioButton!(choice.get() == index, Some(move || choice.set(index)));
        }
        Text!("Count: {}", count.get());
    });
    let name = remember(|| mutable_state_of(String::new()));
    let city = remember(|| mutable_state_of(String::from("Paris")));
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
        modifier = Modifier.padding_horizontal_vertical(0.dp(), 8.dp()),
        horizontal_arrangement = Arrangement::spaced_by(16.dp()),
        vertical_alignment = Alignment::CENTER_VERTICALLY,
        |row: RowScope| {
            CircularProgressIndicator!(level.get());
            LinearProgressIndicator!(level.get(), modifier = Modifier.weight(row, 1.0));
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
