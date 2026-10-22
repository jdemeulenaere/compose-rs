//! A social feed, the compose-rs side of the Android benchmark (android/bench, docs/12): the
//! same screens as `android/bench/compose`'s Kotlin Compose app, widget for widget.
//!
//! - `feed`: a top app bar over a lazy list of 1,000 posts (an avatar, the author, when, the
//!   text, a like button and a tag, in a card).
//! - `feed_animated`: the same, with a header whose spinner, pulsing dot and turning star
//!   animate forever (each read while drawing, as Compose's `graphicsLayer { }`).
//! - `big`: 200 posts in a scrolling column, all composed at once.

#![allow(non_snake_case)]

use compose::animation::*;
use compose::foundation::layout::*;
use compose::foundation::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

/// The posts in the lazy feed.
pub const POSTS: u32 = 1_000;
/// The posts the big screen composes at once.
pub const BIG_POSTS: u32 = 200;

const AUTHORS: [&str; 16] = [
    "Ada Lovelace",
    "Alan Turing",
    "Grace Hopper",
    "Linus Torvalds",
    "Barbara Liskov",
    "Donald Knuth",
    "Margaret Hamilton",
    "Dennis Ritchie",
    "Ken Thompson",
    "Radia Perlman",
    "Edsger Dijkstra",
    "Frances Allen",
    "John McCarthy",
    "Hedy Lamarr",
    "Tim Berners-Lee",
    "Katherine Johnson",
];

const SENTENCES: [&str; 8] = [
    "Shipped a new release today, with faster startup and fewer allocations.",
    "Reading about incremental computation and how UI frameworks skip work.",
    "Our team switched to declarative UI last year and never looked back.",
    "Profiling showed most of the frame was spent measuring text again.",
    "Coffee, a whiteboard, and a long discussion about state hoisting.",
    "Lazy lists only compose what is visible, plus a few items beyond.",
    "Wrote a small benchmark to compare scrolling on two frameworks.",
    "The best optimization is the work you never have to do at all.",
];

const TAGS: [&str; 6] = ["kotlin", "rust", "compose", "android", "performance", "design"];

const COLORS: [u32; 8] = [
    0xFF6750A4, 0xFF7D5260, 0xFF006A6A, 0xFF8C4A00, 0xFF3F51B5, 0xFF00796B, 0xFFC2185B, 0xFF5D4037,
];

/// A post's content, from its index: the Kotlin app derives the same.
#[derive(Clone, PartialEq)]
pub struct Post {
    pub author: &'static str,
    pub initials: String,
    pub hours: u32,
    pub body: String,
    pub likes: u32,
    pub tag: &'static str,
    pub color: Color,
}

impl Post {
    pub fn of(index: u32) -> Post {
        let i = index as usize;
        let author = AUTHORS[i % AUTHORS.len()];
        let initials = author.split(' ').filter_map(|word| word.chars().next()).collect();
        Post {
            author,
            initials,
            hours: (index * 7) % 24 + 1,
            body: format!(
                "{} {}",
                SENTENCES[i % SENTENCES.len()],
                SENTENCES[(i * 3 + 1) % SENTENCES.len()]
            ),
            likes: (index * 37) % 500,
            tag: TAGS[i % TAGS.len()],
            color: Color::from_argb(COLORS[i % COLORS.len()]),
        }
    }
}

#[composable]
pub fn Feed() {
    FeedScreen(false);
}

#[composable]
pub fn FeedAnimated() {
    FeedScreen(true);
}

#[composable]
fn FeedScreen(animated: bool) {
    let color_scheme =
        if is_system_in_dark_theme() { dark_color_scheme() } else { light_color_scheme() };
    MaterialTheme!(color_scheme = color_scheme, {
        Scaffold!(
            top_bar = {
                TopAppBar!({
                    Text!("Feed");
                });
            },
            |padding: PaddingValues| {
                LazyColumn!(
                    modifier = Modifier.padding_values(padding).fill_max_size(),
                    move |scope: LazyListScope| {
                        if animated {
                            scope.item(LiveHeader);
                        }
                        scope.items_keyed(
                            POSTS as usize,
                            |index| index as u64,
                            |index| PostCard(index as u32),
                        );
                    }
                );
            }
        );
    });
}

/// The big screen: 200 posts in a scrolling column, all composed at once.
#[composable]
pub fn BigScreen() {
    let color_scheme =
        if is_system_in_dark_theme() { dark_color_scheme() } else { light_color_scheme() };
    MaterialTheme!(color_scheme = color_scheme, {
        Scaffold!(
            top_bar = {
                TopAppBar!({
                    Text!("Big screen");
                });
            },
            |padding: PaddingValues| {
                let scroll = remember_scroll_state();
                Column!(
                    modifier =
                        Modifier.padding_values(padding).fill_max_size().vertical_scroll(scroll),
                    {
                        for index in 0..BIG_POSTS {
                            key(index, move || PostCard(index));
                        }
                    }
                );
            }
        );
    });
}

/// A header animating forever: a spinner, a pulsing dot and a turning star, their values read
/// while drawing.
#[composable]
fn LiveHeader() {
    let transition = remember_infinite_transition("Live");
    let pulse = transition.animate_float(
        0.2,
        1.0,
        infinite_repeatable(tween(800).easing(LINEAR_EASING), RepeatMode::Reverse),
    );
    let turn = transition.animate_float(
        0.0,
        360.0,
        infinite_repeatable(tween(2000).easing(LINEAR_EASING), RepeatMode::Restart),
    );
    let scheme = MaterialTheme::color_scheme();
    Card!(modifier = Modifier.fill_max_width().padding_horizontal_vertical(16.dp(), 8.dp()), {
        Row!(
            modifier = Modifier.padding(16.dp()),
            horizontal_arrangement = Arrangement::spaced_by(16.dp()),
            vertical_alignment = Alignment::CENTER_VERTICALLY,
            {
                IndeterminateCircularProgressIndicator!(modifier = Modifier.size(24.dp()));
                Box!(
                    modifier = Modifier
                        .size(12.dp())
                        .graphics_layer(move |layer| layer.alpha = pulse.get())
                        .clip(CIRCLE_SHAPE)
                        .background(scheme.error)
                );
                Icon!(
                    icons::filled::star(),
                    None,
                    modifier = Modifier.graphics_layer(move |layer| layer.rotation_z = turn.get()),
                    tint = scheme.primary
                );
                Text!("Live updates", style = MaterialTheme::typography().title_medium);
            }
        );
    });
}

#[composable]
fn PostCard(index: u32) {
    let liked = remember(|| mutable_state_of(false));
    let post = Post::of(index);
    let scheme = MaterialTheme::color_scheme();
    let typography = MaterialTheme::typography();
    Card!(modifier = Modifier.fill_max_width().padding_horizontal_vertical(16.dp(), 4.dp()), {
        Column!(modifier = Modifier.padding(16.dp()), {
            Row!(vertical_alignment = Alignment::CENTER_VERTICALLY, {
                Box!(
                    modifier = Modifier.size(40.dp()).clip(CIRCLE_SHAPE).background(post.color),
                    content_alignment = Alignment::CENTER,
                    {
                        Text!(
                            "{}",
                            post.initials,
                            color = Color::WHITE,
                            style = typography.title_small
                        );
                    }
                );
                Spacer(Modifier.width(12.dp()));
                Column!({
                    Text!("{}", post.author, style = typography.title_small);
                    Text!(
                        "{} h ago",
                        post.hours,
                        style = typography.body_small,
                        color = scheme.on_surface_variant
                    );
                });
            });
            Spacer(Modifier.height(8.dp()));
            Text!(
                "{}",
                post.body,
                style = typography.body_medium,
                max_lines = 3,
                overflow = TextOverflow::Ellipsis
            );
            Row!(vertical_alignment = Alignment::CENTER_VERTICALLY, |row: RowScope| {
                let is_liked = liked.get();
                IconButton!(on_click = move || liked.set(!is_liked), {
                    Icon!(
                        icons::filled::favorite(),
                        Some("Like"),
                        tint = if is_liked { Color::from_argb(0xFFE53935) } else { scheme.outline }
                    );
                });
                Text!("{}", post.likes + u32::from(is_liked), style = typography.label_large);
                Spacer(Modifier.weight(row, 1.0));
                Text!("#{}", post.tag, style = typography.label_large, color = scheme.primary);
            });
        });
    });
}

compose_android::export_apps! {
    "feed" => Feed,
    "feed_animated" => FeedAnimated,
    "big" => BigScreen,
}
