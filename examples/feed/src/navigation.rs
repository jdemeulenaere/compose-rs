//! The navigation screens (docs/17): the latency of navigating to an expensive screen never seen
//! before, the same in `android/bench-compose`'s `Navigation.kt`, widget for widget.
//!
//! - `navigation`: a light launcher, a top app bar over one button. Tapping it switches to a
//!   heavy screen (a plain `if`: no navigation host, no transition), whose first frame composes,
//!   lays out and draws [`NAVIGATION_ITEMS`] varied items at once in a scrolling column: profile
//!   cards (a gradient avatar, a multi-line paragraph, chips), articles (an `AnnotatedString`,
//!   icons with counts), stats (a canvas bar chart, a progress bar, buttons), galleries (images)
//!   and banners (a shadow, a gradient, a canvas sparkline). Back (the key, or the top bar's
//!   button) returns to the launcher, disposing the heavy screen.
//! - Each visit shows new content: the visit's number (`seed`) drives every string, count and
//!   color, its words made of syllables drawn from it ([`NavigationItem::of`]), so no text,
//!   paragraph or composition cache from an earlier visit helps either app.
//! - The heavy screen's composition starts with a trace section, `navigation: heavy screen`
//!   (empty), which measure.py's `navigation` scenario finds the first frame by.

use compose::foundation::layout::*;
use compose::foundation::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

use crate::COLORS;
use crate::stress::{BITMAPS, Counted, bitmap};

/// The items the heavy screen composes at once.
pub const NAVIGATION_ITEMS: u32 = 50;
/// The kinds of items, in turn.
pub const NAVIGATION_KINDS: u32 = 5;
/// The bars of a stats card's chart.
pub const CHART_BARS: u32 = 12;
/// The points of a banner's sparkline.
pub const SPARKLINE_POINTS: u32 = 24;

/// The syllables words are made of: two to four each, drawn from the seed.
pub const SYLLABLES: [&str; 24] = [
    "ka", "lo", "mi", "ra", "ten", "vu", "sel", "dor", "ni", "pa", "qui", "zen", "bo", "fa", "lin",
    "mur", "os", "ta", "ve", "xi", "ro", "sa", "nel", "dri",
];

/// A 32-bit integer hash (lowbias32): the Kotlin app's `mix` computes the same.
pub fn mix(value: u32) -> u32 {
    let mut x = value;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    x
}

/// The `salt`th number of item `index` on visit `seed`, below `bound`.
pub fn pick(seed: u32, index: u32, salt: u32, bound: u32) -> u32 {
    let hash =
        mix(mix(seed.wrapping_mul(0x9E37_79B9) ^ index)
            .wrapping_add(salt.wrapping_mul(0x85EB_CA6B)));
    (hash >> 1) % bound
}

/// An item's numbers, drawn in turn: the Kotlin app's `Draws` draws the same, in the same order.
struct Draws {
    seed: u32,
    index: u32,
    salt: u32,
}

impl Draws {
    fn below(&mut self, bound: u32) -> u32 {
        self.salt += 1;
        pick(self.seed, self.index, self.salt, bound)
    }

    fn word(&mut self) -> String {
        let syllables = 2 + self.below(3);
        let mut word = String::new();
        for _ in 0..syllables {
            word.push_str(SYLLABLES[self.below(SYLLABLES.len() as u32) as usize]);
        }
        word
    }

    fn capitalized(&mut self) -> String {
        capitalize(self.word())
    }

    /// `count` words, the first capitalized, separated by spaces.
    fn phrase(&mut self, count: u32) -> String {
        let mut phrase = self.capitalized();
        for _ in 1..count {
            phrase.push(' ');
            phrase.push_str(&self.word());
        }
        phrase
    }

    /// Six to twelve words and a period.
    fn sentence(&mut self) -> String {
        let count = 6 + self.below(7);
        let mut sentence = self.phrase(count);
        sentence.push('.');
        sentence
    }

    /// `count` sentences, separated by spaces.
    fn paragraph(&mut self, count: u32) -> String {
        let mut paragraph = self.sentence();
        for _ in 1..count {
            paragraph.push(' ');
            paragraph.push_str(&self.sentence());
        }
        paragraph
    }

    fn color(&mut self) -> u32 {
        COLORS[self.below(COLORS.len() as u32) as usize]
    }
}

fn capitalize(word: String) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => word,
    }
}

/// What an item of the heavy screen is, from its index.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NavigationKind {
    Profile,
    Article,
    Stats,
    Gallery,
    Banner,
}

impl NavigationKind {
    pub fn of(index: u32) -> NavigationKind {
        match index % NAVIGATION_KINDS {
            0 => NavigationKind::Profile,
            1 => NavigationKind::Article,
            2 => NavigationKind::Stats,
            3 => NavigationKind::Gallery,
            _ => NavigationKind::Banner,
        }
    }
}

/// An item's content on visit `seed`: what its kind shows, drawn from the seed and its index
/// (the Kotlin app's `NavigationItem.of` draws the same, in the same order).
#[derive(Clone, PartialEq, Debug)]
pub enum NavigationItem {
    Profile {
        name: String,
        initials: String,
        handle: String,
        followers: u32,
        bio: String,
        chips: [String; 3],
        selected: bool,
        liked: bool,
        colors: [u32; 2],
    },
    Article {
        title: String,
        author: String,
        minutes: u32,
        lead: String,
        quote: String,
        tag: String,
        highlight: String,
        counts: [u32; 4],
    },
    Stats {
        label: String,
        value: u32,
        delta: u32,
        up: bool,
        bars: [u32; CHART_BARS as usize],
        progress: u32,
        color: u32,
    },
    Gallery {
        album: String,
        images: [u32; 3],
        caption: String,
    },
    Banner {
        title: String,
        line: String,
        points: [u32; SPARKLINE_POINTS as usize],
        colors: [u32; 3],
    },
}

impl NavigationItem {
    pub fn of(seed: u32, index: u32) -> NavigationItem {
        let mut draws = Draws { seed, index, salt: 0 };
        match NavigationKind::of(index) {
            NavigationKind::Profile => {
                let first = draws.capitalized();
                let last = draws.capitalized();
                let initials: String =
                    [&first, &last].iter().filter_map(|word| word.chars().next()).collect();
                let handle = format!("@{}", draws.word());
                let followers = draws.below(5_000);
                let sentences = 2 + draws.below(2);
                let bio = draws.paragraph(sentences);
                let chips = [draws.capitalized(), draws.capitalized(), draws.capitalized()];
                let selected = draws.below(2) == 1;
                let liked = draws.below(2) == 1;
                let colors = [draws.color(), draws.color()];
                NavigationItem::Profile {
                    name: format!("{first} {last}"),
                    initials,
                    handle,
                    followers,
                    bio,
                    chips,
                    selected,
                    liked,
                    colors,
                }
            }
            NavigationKind::Article => {
                let words = 3 + draws.below(4);
                let title = draws.phrase(words);
                let author = draws.phrase(2);
                let minutes = 2 + draws.below(15);
                let lead = draws.phrase(2);
                let quote = draws.sentence();
                let tag = draws.word();
                let highlight = draws.sentence();
                let counts = [
                    draws.below(1_000),
                    draws.below(1_000),
                    draws.below(1_000),
                    draws.below(1_000),
                ];
                NavigationItem::Article {
                    title,
                    author,
                    minutes,
                    lead,
                    quote,
                    tag,
                    highlight,
                    counts,
                }
            }
            NavigationKind::Stats => {
                let label = draws.phrase(2);
                let value = draws.below(100_000);
                let delta = draws.below(2_000);
                let up = draws.below(2) == 1;
                let mut bars = [0; CHART_BARS as usize];
                for bar in &mut bars {
                    *bar = 5 + draws.below(96);
                }
                let progress = draws.below(101);
                let color = draws.color();
                NavigationItem::Stats { label, value, delta, up, bars, progress, color }
            }
            NavigationKind::Gallery => {
                let album = format!("Album {}", draws.capitalized());
                let images = [draws.below(BITMAPS), draws.below(BITMAPS), draws.below(BITMAPS)];
                let caption = draws.sentence();
                NavigationItem::Gallery { album, images, caption }
            }
            NavigationKind::Banner => {
                let title = draws.phrase(2);
                let line = draws.sentence();
                let mut points = [0; SPARKLINE_POINTS as usize];
                for point in &mut points {
                    *point = draws.below(101);
                }
                let colors = [draws.color(), draws.color(), draws.color()];
                NavigationItem::Banner { title, line, points, colors }
            }
        }
    }
}

/// The navigation app: the launcher, or the heavy screen of the visit `seed` while it shows.
#[composable]
pub fn NavigationApp() {
    let color_scheme = crate::bench_color_scheme();
    let seed = remember(|| mutable_state_of(0u32));
    let showing = remember(|| mutable_state_of(false));
    MaterialTheme!(color_scheme = color_scheme, {
        if showing.get() {
            BackHandler!(on_back = move || showing.set(false));
            HeavyScreen(seed.get(), showing);
        } else {
            Launcher(seed, showing);
        }
    });
}

/// A top app bar over one big button, centered, which opens the next visit's heavy screen:
/// measure.py taps the screen's center.
#[composable]
fn Launcher(seed: MutableState<u32>, showing: MutableState<bool>) {
    let typography = MaterialTheme::typography();
    let visits = seed.get();
    Scaffold!(
        top_bar = {
            TopAppBar!(
                {
                    Text!("Navigation");
                },
                window_insets = crate::top_app_bar_window_insets()
            );
        },
        content_window_insets = crate::scaffold_content_window_insets(),
        |padding: PaddingValues| {
            Box!(
                modifier = Modifier.padding(padding).fill_max_size(),
                content_alignment = Alignment::CENTER,
                {
                    Button!(
                        move || {
                            seed.set(seed.get() + 1);
                            showing.set(true);
                        },
                        modifier = Modifier.size((320.dp(), 240.dp())),
                        {
                            Column!(horizontal_alignment = Alignment::CENTER_HORIZONTALLY, {
                                Text!("Open screen {}", visits + 1, style = typography.title_large);
                                Spacer(Modifier.height(8.dp()));
                                Text!("{} visits", visits, style = typography.body_medium);
                            });
                        }
                    );
                }
            );
        }
    );
}

/// The heavy screen: a top app bar with a back button over [`NAVIGATION_ITEMS`] items in a
/// scrolling column, all composed at once.
#[composable]
fn HeavyScreen(seed: u32, showing: MutableState<bool>) {
    // An empty section, where the heavy screen's first composition starts: measure.py finds the
    // frame drawing it by it.
    drop(compose::ui::platform::trace::section(c"navigation: heavy screen"));
    Scaffold!(
        top_bar = {
            TopAppBar!(
                {
                    Text!("Screen {}", seed);
                },
                navigation_icon = Some(move || {
                    IconButton!(move || showing.set(false), {
                        Icon!(icons::filled::arrow_back(), Some("Back"));
                    });
                }),
                window_insets = crate::top_app_bar_window_insets()
            );
        },
        content_window_insets = crate::scaffold_content_window_insets(),
        |padding: PaddingValues| {
            let scroll = remember_scroll_state!();
            Column!(
                modifier = Modifier.padding(padding).fill_max_size().vertical_scroll(scroll),
                {
                    for index in 0..NAVIGATION_ITEMS {
                        key(index, move || HeavyItem(seed, index));
                    }
                }
            );
        }
    );
}

#[composable]
fn HeavyItem(seed: u32, index: u32) {
    match NavigationKind::of(index) {
        NavigationKind::Profile => Profile(seed, index),
        NavigationKind::Article => Article(seed, index),
        NavigationKind::Stats => Stats(seed, index),
        NavigationKind::Gallery => Gallery(seed, index),
        NavigationKind::Banner => Banner(seed, index),
    }
}

/// An elevated card: a gradient avatar, a name and a handle, a like button, a paragraph of two
/// or three sentences, and three chips.
#[composable]
fn Profile(seed: u32, index: u32) {
    let NavigationItem::Profile {
        name,
        initials,
        handle,
        followers,
        bio,
        chips,
        selected,
        liked,
        colors,
    } = NavigationItem::of(seed, index)
    else {
        unreachable!("a profile's index")
    };
    let scheme = MaterialTheme::color_scheme();
    let typography = MaterialTheme::typography();
    let [first_chip, second_chip, third_chip] = chips;
    ElevatedCard!(modifier = Modifier.fill_max_width().padding((16.dp(), 6.dp())), {
        Column!(modifier = Modifier.padding(16.dp()), {
            Row!(vertical_alignment = Alignment::CENTER_VERTICALLY, |row: RowScope| {
                Box!(
                    modifier = Modifier.size(48.dp()).clip(CIRCLE_SHAPE).background(
                        Brush::linear_gradient([
                            Color::from_argb(colors[0]),
                            Color::from_argb(colors[1])
                        ])
                    ),
                    content_alignment = Alignment::CENTER,
                    {
                        Text!(
                            "{}",
                            initials,
                            color = Color::WHITE,
                            style = typography.title_medium
                        );
                    }
                );
                Spacer(Modifier.width(12.dp()));
                Column!(modifier = Modifier.weight(row, 1.0), {
                    Text!("{}", name, style = typography.title_medium);
                    Text!(
                        "{} · {} followers",
                        handle,
                        followers,
                        style = typography.body_small,
                        color = scheme.on_surface_variant
                    );
                });
                IconButton!(|| {}, {
                    Icon!(
                        icons::filled::favorite(),
                        Some("Like"),
                        tint = if liked { Color::from_argb(0xFFE53935) } else { scheme.outline }
                    );
                });
            });
            Spacer(Modifier.height(8.dp()));
            Text!("{}", bio, style = typography.body_medium);
            Spacer(Modifier.height(8.dp()));
            Row!(horizontal_arrangement = Arrangement::spaced_by(8.dp()), {
                AssistChip!(
                    || {},
                    move || {
                        Text!("{}", first_chip);
                    },
                    leading_icon = Some(|| {
                        Icon!(icons::filled::place(), None, modifier = Modifier.size(18.dp()));
                    })
                );
                FilterChip!(selected, || {}, move || {
                    Text!("{}", second_chip);
                });
                SuggestionChip!(|| {}, move || {
                    Text!("{}", third_chip);
                });
            });
        });
    });
}

/// A card: a title, a byline, a styled paragraph (bold, italic, a colored tag, an underlined
/// link, a highlight), and four icons with counts.
#[composable]
fn Article(seed: u32, index: u32) {
    let NavigationItem::Article { title, author, minutes, lead, quote, tag, highlight, counts } =
        NavigationItem::of(seed, index)
    else {
        unreachable!("an article's index")
    };
    let scheme = MaterialTheme::color_scheme();
    let typography = MaterialTheme::typography();
    let body = AnnotatedString::build(|text| {
        let bold = SpanStyle { font_weight: Some(FontWeight::BOLD), ..SpanStyle::DEFAULT };
        text.with_style(bold, |text| {
            text.append(&lead);
        });
        text.append(" says: ");
        let italic = SpanStyle { font_style: Some(FontStyle::Italic), ..SpanStyle::DEFAULT };
        text.with_style(italic, |text| {
            text.append(&quote);
        });
        text.append(" ");
        let tag_style = SpanStyle {
            color: scheme.primary,
            font_weight: Some(FontWeight::MEDIUM),
            ..SpanStyle::DEFAULT
        };
        text.with_style(tag_style, |text| {
            text.append("#");
            text.append(&tag);
        });
        text.append(" ");
        let link = SpanStyle {
            color: scheme.tertiary,
            text_decoration: Some(TextDecoration::UNDERLINE),
            ..SpanStyle::DEFAULT
        };
        text.with_style(link, |text| {
            text.append("Read more");
        });
        text.append(" ");
        let marked = SpanStyle { background: Color::from_argb(0x55FFC107), ..SpanStyle::DEFAULT };
        text.with_style(marked, |text| {
            text.append(&highlight);
        });
    });
    Card!(modifier = Modifier.fill_max_width().padding((16.dp(), 6.dp())), {
        Column!(modifier = Modifier.padding(16.dp()), {
            Text!("{}", title, style = typography.title_large);
            Spacer(Modifier.height(4.dp()));
            Text!(
                "{} · {} min read",
                author,
                minutes,
                style = typography.label_medium,
                color = scheme.on_surface_variant
            );
            Spacer(Modifier.height(8.dp()));
            AnnotatedText!(body.clone(), style = typography.body_medium);
            Spacer(Modifier.height(12.dp()));
            Row!(
                horizontal_arrangement = Arrangement::spaced_by(16.dp()),
                vertical_alignment = Alignment::CENTER_VERTICALLY,
                {
                    Counted(icons::filled::thumb_up(), counts[0]);
                    Counted(icons::filled::share(), counts[1]);
                    Counted(icons::filled::star(), counts[2]);
                    Counted(icons::filled::send(), counts[3]);
                }
            );
        });
    });
}

/// An outlined card: a label, a value and its change beside a canvas bar chart, a progress bar,
/// and two buttons.
#[composable]
fn Stats(seed: u32, index: u32) {
    let NavigationItem::Stats { label, value, delta, up, bars, progress, color } =
        NavigationItem::of(seed, index)
    else {
        unreachable!("a stats card's index")
    };
    let scheme = MaterialTheme::color_scheme();
    let typography = MaterialTheme::typography();
    let fill = Color::from_argb(color);
    OutlinedCard!(modifier = Modifier.fill_max_width().padding((16.dp(), 6.dp())), {
        Column!(
            modifier = Modifier.padding(16.dp()),
            vertical_arrangement = Arrangement::spaced_by(8.dp()),
            {
                Row!(vertical_alignment = Alignment::CENTER_VERTICALLY, |row: RowScope| {
                    Column!(modifier = Modifier.weight(row, 1.0), {
                        Text!("{}", label, style = typography.label_large);
                        Text!("{}", value, style = typography.headline_small);
                        Text!(
                            "{}{}.{} %",
                            if up { "+" } else { "-" },
                            delta / 10,
                            delta % 10,
                            style = typography.label_medium,
                            color = if up { scheme.primary } else { scheme.error }
                        );
                    });
                    Canvas(Modifier.size((132.dp(), 56.dp())), move |scope| {
                        let size = scope.size();
                        let gap = scope.density().to_px(3.dp());
                        let width =
                            (size.width - gap * (CHART_BARS - 1) as f32) / CHART_BARS as f32;
                        let radius = scope.density().to_px(2.dp());
                        for (bar, percent) in bars.iter().enumerate() {
                            let height = size.height * *percent as f32 / 100.0;
                            let left = bar as f32 * (width + gap);
                            scope.draw_round_rect(
                                Rect::new(left, size.height - height, left + width, size.height),
                                [radius; 4],
                                Paint::fill(fill.with_alpha(0.35 + 0.65 * *percent as f32 / 100.0)),
                            );
                        }
                    });
                });
                // Without Material 3 1.4's gap, as Navigation.kt's (which also leaves out the
                // stop dot), as the stress screen's.
                LinearProgressIndicator!(
                    progress as f32 / 100.0,
                    modifier = Modifier.fill_max_width(),
                    gap_size = 0.dp()
                );
                Row!(
                    horizontal_arrangement = Arrangement::spaced_by(8.dp()),
                    vertical_alignment = Alignment::CENTER_VERTICALLY,
                    |row: RowScope| {
                        Button!(|| {}, {
                            Text!("Open");
                        });
                        OutlinedButton!(|| {}, {
                            Text!("Share");
                        });
                        Spacer(Modifier.weight(row, 1.0));
                        Text!("{} %", progress, style = typography.label_large);
                    }
                );
            }
        );
    });
}

/// A title and an icon over three images and a caption.
#[composable]
fn Gallery(seed: u32, index: u32) {
    let NavigationItem::Gallery { album, images, caption } = NavigationItem::of(seed, index) else {
        unreachable!("a gallery's index")
    };
    let typography = MaterialTheme::typography();
    Column!(modifier = Modifier.fill_max_width().padding((16.dp(), 8.dp())), {
        Row!(vertical_alignment = Alignment::CENTER_VERTICALLY, |row: RowScope| {
            Text!(
                "{}",
                album,
                style = typography.title_medium,
                modifier = Modifier.weight(row, 1.0)
            );
            Icon!(icons::filled::arrow_forward(), Some("More"));
        });
        Spacer(Modifier.height(8.dp()));
        Row!(horizontal_arrangement = Arrangement::spaced_by(8.dp()), {
            for image in images {
                Image!(
                    bitmap(image),
                    None,
                    modifier = Modifier.size((120.dp(), 90.dp())).clip(RoundedCornerShape(12.dp())),
                    content_scale = ContentScale::Crop
                );
            }
        });
        Spacer(Modifier.height(4.dp()));
        Text!(
            "{}",
            caption,
            style = typography.body_small,
            max_lines = 2,
            overflow = TextOverflow::Ellipsis
        );
    });
}

/// A gradient box with a shadow, a sparkline drawn on a canvas, a title and a line over it, and
/// icons in a corner.
#[composable]
fn Banner(seed: u32, index: u32) {
    let NavigationItem::Banner { title, line, points, colors } = NavigationItem::of(seed, index)
    else {
        unreachable!("a banner's index")
    };
    let shape = RoundedCornerShape(16.dp());
    let typography = MaterialTheme::typography();
    // One path, reset for each drawing, as Navigation.kt's.
    let path = remember(Path::new);
    Box!(
        modifier = Modifier
            .fill_max_width()
            .padding((16.dp(), 6.dp()))
            .height(140.dp())
            .shadow_with(6.dp(), shape, true, DEFAULT_SHADOW_COLOR, DEFAULT_SHADOW_COLOR)
            .background(Brush::linear_gradient([
                Color::from_argb(colors[0]),
                Color::from_argb(colors[1]),
                Color::from_argb(colors[2])
            ])),
        |scope: BoxScope| {
            Canvas(Modifier.fill_max_size(), move |scope| {
                let size = scope.size();
                let stroke = scope.density().to_px(2.dp());
                let dot = scope.density().to_px(3.dp());
                let step = size.width / (SPARKLINE_POINTS - 1) as f32;
                path.reset();
                for (point, percent) in points.iter().enumerate() {
                    let x = point as f32 * step;
                    let y = size.height * (0.15 + 0.5 * (1.0 - *percent as f32 / 100.0));
                    if point == 0 {
                        path.move_to(x, y);
                    } else {
                        path.line_to(x, y);
                    }
                }
                scope.draw_path(&path, Paint::stroke(Color::WHITE.with_alpha(0.7), stroke));
                for (point, percent) in points.iter().enumerate() {
                    if point % 4 == 0 {
                        let x = point as f32 * step;
                        let y = size.height * (0.15 + 0.5 * (1.0 - *percent as f32 / 100.0));
                        scope.draw_circle(Offset::new(x, y), dot, Paint::fill(Color::WHITE));
                    }
                }
            });
            Column!(modifier = Modifier.align(scope, Alignment::BOTTOM_START).padding(16.dp()), {
                Text!("{}", title, style = typography.title_large, color = Color::WHITE);
                Text!(
                    "{}",
                    line,
                    style = typography.body_medium,
                    color = Color::WHITE,
                    max_lines = 1,
                    overflow = TextOverflow::Ellipsis
                );
            });
            Row!(
                modifier = Modifier.align(scope, Alignment::TOP_END).padding(12.dp()),
                horizontal_arrangement = Arrangement::spaced_by(8.dp()),
                {
                    Icon!(icons::filled::star(), None, tint = Color::WHITE);
                    Icon!(icons::filled::share(), None, tint = Color::WHITE);
                }
            );
        }
    );
}
