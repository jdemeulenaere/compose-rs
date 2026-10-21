//! A tour of the toolkit: a scaffold with a top app bar and a floating action button, Material's
//! selection controls, cards, icons, an animated panel and a lazy list.

#![allow(non_snake_case)]

use compose::animation::*;
use compose::foundation::layout::*;
use compose::foundation::{LazyColumn, LazyListScope};
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

#[composable]
pub fn Gallery() {
    MaterialTheme!({
        let count = remember(|| mutable_state_of(0));
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
                        IconButton!(on_click = || {}, {
                            Icon!(icons::filled::search(), Some("Search"));
                        });
                        IconButton!(on_click = || {}, {
                            Icon!(icons::filled::more_vert(), Some("More"));
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
                Column!(
                    modifier = Modifier.padding_values(padding).padding(16.dp()).fill_max_size(),
                    vertical_arrangement = Arrangement::spaced_by(12.dp()),
                    |column| {
                        Controls(count);
                        LazyColumn!(
                            modifier = Modifier.weight(column, 1.0).fill_max_width(),
                            vertical_arrangement = Arrangement::spaced_by(8.dp()),
                            |scope: LazyListScope| {
                                scope.items(100, |index| {
                                    Card!(modifier = Modifier.fill_max_width(), {
                                        Row!(
                                            modifier = Modifier.padding(12.dp()),
                                            vertical_alignment = Alignment::CENTER_VERTICALLY,
                                            {
                                                Icon!(icons::filled::star(), None);
                                                Text!("  Card {}", index);
                                            }
                                        );
                                    });
                                });
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
    Slider!(level.get(), move |value| level.set(value));
    AnimatedVisibility!(switched.get(), {
        Card!(modifier = Modifier.fill_max_width(), {
            Text!("Shown while the switch is on", modifier = Modifier.padding(12.dp()));
        });
    });
    HorizontalDivider!();
}
