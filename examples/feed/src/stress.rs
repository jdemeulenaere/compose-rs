//! The stress screens (docs/15): much heavier than the feed, the same in `android/bench-compose`'s
//! `Stress.kt`, widget for widget.
//!
//! - `stress`: a lazy list of 1,000 items of four kinds, under a sticky header every 20 items:
//!   carousels (a lazy row of image tiles: procedural bitmaps), rich posts (an elevated card
//!   with a gradient avatar, styled text and icons), action cards (chips, buttons and a progress
//!   bar) and banners (a gradient box with a shadow and icons over it).
//! - `stress_animation`: many animations at once: a grid of 140 cells each turning, scaling,
//!   fading and changing color forever (read while drawing), a text changing every frame, bars
//!   whose widths animate (read while measuring) beside percentages (recomposed), a card
//!   growing and shrinking (`animate_content_size`), and a canvas of 50 sine waves drawn anew
//!   each frame.

use std::f32::consts::PI;
use std::time::Duration;

use compose::animation::*;
use compose::foundation::layout::*;
use compose::foundation::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

use crate::{AUTHORS, COLORS, SENTENCES, TAGS};

/// The items of the stress list.
pub const STRESS_ITEMS: u32 = 1_000;
/// A sticky header before every this many items.
pub const SECTION_ITEMS: u32 = 20;
/// The tiles of a carousel.
pub const TILES: u32 = 12;
/// The procedural bitmaps the tiles show.
pub const BITMAPS: u32 = 8;
/// Their size in pixels.
pub const BITMAP_WIDTH: u32 = 240;
pub const BITMAP_HEIGHT: u32 = 180;
/// The animated grid: its rows and columns.
pub const GRID_ROWS: u32 = 10;
pub const GRID_COLUMNS: u32 = 14;
/// The animated bars.
pub const BARS: u32 = 6;
/// The canvas's sine waves.
pub const WAVES: u32 = 50;
/// The points of each wave.
pub const WAVE_SEGMENTS: u32 = 48;

/// Bitmap `which`'s pixel at (`x`, `y`), as 0xAARRGGBB: a diagonal gradient between two of the
/// palette's colors with lighter rings around a point that moves with `which`. Integer
/// arithmetic only, so that the Kotlin app computes the same pixels.
pub fn bitmap_pixel(which: u32, x: u32, y: u32) -> u32 {
    let from = COLORS[(which % 8) as usize];
    let to = COLORS[((which + 3) % 8) as usize];
    let span = BITMAP_WIDTH + BITMAP_HEIGHT - 2;
    let along = x + y;
    let center_x = (40 + which * 23 % 160) as i32;
    let center_y = (30 + which * 37 % 120) as i32;
    let (dx, dy) = (x as i32 - center_x, y as i32 - center_y);
    let lift = if ((dx * dx + dy * dy) as u32 / 96) % 12 < 4 { 48 } else { 0 };
    let channel = |shift: u32| {
        let (a, b) = ((from >> shift) & 0xFF, (to >> shift) & 0xFF);
        ((a * (span - along) + b * along) / span + lift).min(255)
    };
    0xFF00_0000 | channel(16) << 16 | channel(8) << 8 | channel(0)
}

/// Bitmap `which`, as an `ImageBitmap` (opaque, so premultiplied is straight).
pub fn stress_bitmap(which: u32) -> ImageBitmap {
    let mut pixels = Vec::with_capacity((BITMAP_WIDTH * BITMAP_HEIGHT * 4) as usize);
    for y in 0..BITMAP_HEIGHT {
        for x in 0..BITMAP_WIDTH {
            let argb = bitmap_pixel(which, x, y);
            pixels.extend_from_slice(&[(argb >> 16) as u8, (argb >> 8) as u8, argb as u8, 0xFF]);
        }
    }
    ImageBitmap::from_rgba_premultiplied(BITMAP_WIDTH, BITMAP_HEIGHT, pixels)
}

thread_local! {
    /// The bitmaps, made the first time a tile asks (as the Kotlin app's `by lazy`).
    static STRESS_BITMAPS: Vec<ImageBitmap> = (0..BITMAPS).map(stress_bitmap).collect();
}

fn bitmap(which: u32) -> ImageBitmap {
    STRESS_BITMAPS.with(|bitmaps| bitmaps[(which % BITMAPS) as usize].clone())
}

/// What an item of the stress list is, from its index; also its content type.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum StressKind {
    Carousel,
    RichPost,
    ActionCard,
    Banner,
}

impl StressKind {
    pub fn of(index: u32) -> StressKind {
        match index % 4 {
            0 => StressKind::Carousel,
            1 => StressKind::RichPost,
            2 => StressKind::ActionCard,
            _ => StressKind::Banner,
        }
    }
}

fn color(index: u32) -> Color {
    Color::from_argb(COLORS[(index % 8) as usize])
}

/// The stress list: a top app bar over 1,000 items of four kinds, a sticky header before every
/// 20.
#[composable]
pub fn StressScroll() {
    let color_scheme =
        if is_system_in_dark_theme() { dark_color_scheme() } else { light_color_scheme() };
    MaterialTheme!(color_scheme = color_scheme, {
        Scaffold!(
            top_bar = {
                TopAppBar!({
                    Text!("Stress");
                });
            },
            |padding: PaddingValues| {
                LazyColumn!(
                    modifier = Modifier.padding(padding).fill_max_size(),
                    move |scope: LazyListScope| {
                        for section in 0..STRESS_ITEMS / SECTION_ITEMS {
                            // Headers' keys apart from the items' (the Kotlin app's are strings).
                            // Content types as the Kotlin app's: an item reuses one of its kind.
                            scope.sticky_header_keyed_with(
                                STRESS_ITEMS as u64 + section as u64,
                                "header",
                                move |_| SectionHeader(section),
                            );
                            let first = section * SECTION_ITEMS;
                            scope.items_keyed_with(
                                SECTION_ITEMS as usize,
                                move |offset| (first + offset as u32) as u64,
                                move |offset| StressKind::of(first + offset as u32),
                                move |offset| StressItem(first + offset as u32),
                            );
                        }
                    }
                );
            }
        );
    });
}

#[composable]
fn SectionHeader(section: u32) {
    let scheme = MaterialTheme::color_scheme();
    Box!(
        modifier = Modifier
            .fill_max_width()
            .background(scheme.secondary_container)
            .padding((16.dp(), 8.dp())),
        {
            Text!(
                "Section {}",
                section + 1,
                style = MaterialTheme::typography().title_small,
                color = scheme.on_secondary_container
            );
        }
    );
}

#[composable]
fn StressItem(index: u32) {
    match StressKind::of(index) {
        StressKind::Carousel => Carousel(index),
        StressKind::RichPost => RichPost(index),
        StressKind::ActionCard => ActionCard(index),
        StressKind::Banner => Banner(index),
    }
}

/// A title over a lazy row of image tiles.
#[composable]
fn Carousel(index: u32) {
    Column!(modifier = Modifier.fill_max_width().padding((0.dp(), 8.dp())), {
        Row!(
            modifier = Modifier.fill_max_width().padding((16.dp(), 4.dp())),
            vertical_alignment = Alignment::CENTER_VERTICALLY,
            |row: RowScope| {
                Text!(
                    "Collection {}",
                    index,
                    style = MaterialTheme::typography().title_medium,
                    modifier = Modifier.weight(row, 1.0)
                );
                Icon!(icons::filled::arrow_forward(), Some("More"));
            }
        );
        LazyRow!(
            content_padding = PaddingValues::symmetric(16.dp(), 0.dp()),
            horizontal_arrangement = Arrangement::spaced_by(8.dp()),
            move |scope: LazyListScope| {
                scope.items_keyed(
                    TILES as usize,
                    |tile| tile as u64,
                    move |tile| Tile(index, tile as u32),
                );
            }
        );
    });
}

#[composable]
fn Tile(index: u32, tile: u32) {
    Column!(modifier = Modifier.width(120.dp()), {
        Image!(
            bitmap(index + tile),
            None,
            modifier = Modifier.size((120.dp(), 90.dp())).clip(RoundedCornerShape(12.dp())),
            content_scale = ContentScale::Crop
        );
        Text!(
            "Tile {}",
            tile + 1,
            style = MaterialTheme::typography().body_small,
            max_lines = 1,
            modifier = Modifier.padding((0.dp(), 4.dp()))
        );
    });
}

/// An elevated card: a gradient avatar, the author, styled text, and a row of icons with counts.
#[composable]
fn RichPost(index: u32) {
    let i = index as usize;
    let author = AUTHORS[i % AUTHORS.len()];
    let scheme = MaterialTheme::color_scheme();
    let typography = MaterialTheme::typography();
    let body = remember_keyed(index, move || {
        let first_name = author.split(' ').next().unwrap_or(author);
        AnnotatedString::build(|text| {
            let bold = SpanStyle { font_weight: Some(FontWeight::BOLD), ..SpanStyle::DEFAULT };
            text.with_style(bold, |text| {
                text.append(first_name);
            });
            text.append(" wrote: ");
            let italic = SpanStyle { font_style: Some(FontStyle::Italic), ..SpanStyle::DEFAULT };
            text.with_style(italic, |text| {
                text.append(SENTENCES[i % SENTENCES.len()]);
            });
            text.append(" ");
            let tag = SpanStyle {
                color: scheme.primary,
                font_weight: Some(FontWeight::MEDIUM),
                ..SpanStyle::DEFAULT
            };
            text.with_style(tag, |text| {
                text.append("#");
                text.append(TAGS[i % TAGS.len()]);
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
            let highlight =
                SpanStyle { background: Color::from_argb(0x55FFC107), ..SpanStyle::DEFAULT };
            text.with_style(highlight, |text| {
                text.append(SENTENCES[(i * 3 + 1) % SENTENCES.len()]);
            });
        })
    });
    ElevatedCard!(modifier = Modifier.fill_max_width().padding((16.dp(), 6.dp())), {
        Column!(modifier = Modifier.padding(16.dp()), {
            Row!(vertical_alignment = Alignment::CENTER_VERTICALLY, |row: RowScope| {
                Box!(
                    modifier = Modifier
                        .size(44.dp())
                        .clip(CIRCLE_SHAPE)
                        .background(Brush::linear_gradient([color(index), color(index + 3)])),
                    content_alignment = Alignment::CENTER,
                    {
                        Text!(
                            "{}",
                            index % 100,
                            color = Color::WHITE,
                            style = typography.title_small
                        );
                    }
                );
                Spacer(Modifier.width(12.dp()));
                Column!(modifier = Modifier.weight(row, 1.0), {
                    Text!("{}", author, style = typography.title_small);
                    Text!(
                        "{} h ago",
                        (index * 7) % 24 + 1,
                        style = typography.body_small,
                        color = scheme.on_surface_variant
                    );
                });
                Icon!(icons::filled::more_vert(), Some("More"));
            });
            Spacer(Modifier.height(8.dp()));
            AnnotatedText!(
                body.clone(),
                style = typography.body_medium,
                max_lines = 4,
                overflow = TextOverflow::Ellipsis
            );
            Spacer(Modifier.height(8.dp()));
            Row!(
                horizontal_arrangement = Arrangement::spaced_by(16.dp()),
                vertical_alignment = Alignment::CENTER_VERTICALLY,
                {
                    Counted(icons::filled::thumb_up(), (index * 37) % 500);
                    Counted(icons::filled::share(), (index * 11) % 90);
                    Counted(icons::filled::star(), (index * 5) % 40);
                    Counted(icons::filled::send(), (index * 3) % 20);
                }
            );
        });
    });
}

#[composable]
fn Counted(icon: ImageVector, count: u32) {
    Row!(vertical_alignment = Alignment::CENTER_VERTICALLY, {
        Icon!(
            icon,
            None,
            modifier = Modifier.size(18.dp()),
            tint = MaterialTheme::color_scheme().on_surface_variant
        );
        Spacer(Modifier.width(4.dp()));
        Text!("{}", count, style = MaterialTheme::typography().label_medium);
    });
}

/// An outlined card: a title, three chips, a progress bar, and two buttons.
#[composable]
fn ActionCard(index: u32) {
    let progress = (index * 13 % 100) as f32 / 100.0;
    let typography = MaterialTheme::typography();
    OutlinedCard!(modifier = Modifier.fill_max_width().padding((16.dp(), 6.dp())), {
        Column!(
            modifier = Modifier.padding(16.dp()),
            vertical_arrangement = Arrangement::spaced_by(8.dp()),
            {
                Text!("Task {}", index, style = typography.title_small);
                Row!(horizontal_arrangement = Arrangement::spaced_by(8.dp()), {
                    AssistChip!(
                        || {},
                        {
                            Text!("Assist");
                        },
                        leading_icon = Some(|| {
                            Icon!(icons::filled::build(), None, modifier = Modifier.size(18.dp()));
                        })
                    );
                    FilterChip!(index.is_multiple_of(3), || {}, {
                        Text!("Filter");
                    });
                    SuggestionChip!(|| {}, {
                        Text!("Hint");
                    });
                });
                LinearProgressIndicator!(progress, modifier = Modifier.fill_max_width());
                Row!(
                    horizontal_arrangement = Arrangement::spaced_by(8.dp()),
                    vertical_alignment = Alignment::CENTER_VERTICALLY,
                    |row: RowScope| {
                        Button!(|| {}, {
                            Text!("Accept");
                        });
                        OutlinedButton!(|| {}, {
                            Text!("Later");
                        });
                        Spacer(Modifier.weight(row, 1.0));
                        Text!("{} %", index * 13 % 100, style = typography.label_large);
                    }
                );
            }
        );
    });
}

/// A gradient box with a shadow, a title and a line of text over it, and icons in a corner.
#[composable]
fn Banner(index: u32) {
    let shape = RoundedCornerShape(16.dp());
    let typography = MaterialTheme::typography();
    Box!(
        modifier = Modifier
            .fill_max_width()
            .padding((16.dp(), 6.dp()))
            .height(120.dp())
            .shadow_with(6.dp(), shape, true, DEFAULT_SHADOW_COLOR, DEFAULT_SHADOW_COLOR)
            .background(Brush::linear_gradient([color(index), color(index + 2), color(index + 5)])),
        |scope: BoxScope| {
            Column!(modifier = Modifier.align(scope, Alignment::BOTTOM_START).padding(16.dp()), {
                Text!("Featured {}", index, style = typography.title_large, color = Color::WHITE);
                Text!(
                    "{}",
                    SENTENCES[(index as usize * 5) % SENTENCES.len()],
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
                    Icon!(icons::filled::favorite(), None, tint = Color::WHITE);
                    Icon!(icons::filled::share(), None, tint = Color::WHITE);
                    Icon!(icons::filled::star(), None, tint = Color::WHITE);
                }
            );
        }
    );
}

/// The stress animation: a ticking text, a grid of animated cells, animated bars, a card
/// growing and shrinking, and a canvas of sine waves.
#[composable]
pub fn StressAnimation() {
    let color_scheme =
        if is_system_in_dark_theme() { dark_color_scheme() } else { light_color_scheme() };
    MaterialTheme!(color_scheme = color_scheme, {
        Scaffold!(
            top_bar = {
                TopAppBar!({
                    Text!("Stress animation");
                });
            },
            |padding: PaddingValues| {
                Column!(
                    modifier = Modifier.padding(padding).fill_max_size().padding(16.dp()),
                    vertical_arrangement = Arrangement::spaced_by(12.dp()),
                    {
                        Ticker();
                        Grid();
                        Bars();
                        ExpandingCard();
                        Waves();
                    }
                );
            }
        );
    });
}

/// A text changing every frame: the frames since it started, and the milliseconds.
#[composable]
fn Ticker() {
    let frames = remember(|| mutable_state_of(0u32));
    let millis = remember(|| mutable_state_of(0u64));
    LaunchedEffect((), async move {
        let start = with_frame_millis(|time| time).await;
        loop {
            with_frame_millis(move |time| {
                frames.update(|frames| frames + 1);
                millis.set(time - start);
            })
            .await;
        }
    });
    Text!(
        "Frame {} · {} ms",
        frames.get(),
        millis.get(),
        style = MaterialTheme::typography().title_medium
    );
}

#[composable]
fn Grid() {
    Column!(vertical_arrangement = Arrangement::spaced_by(6.dp()), {
        for row in 0..GRID_ROWS {
            Row!(
                modifier = Modifier.fill_max_width(),
                horizontal_arrangement = Arrangement::SPACE_BETWEEN,
                {
                    for column in 0..GRID_COLUMNS {
                        let index = row * GRID_COLUMNS + column;
                        key(index, move || Cell(index));
                    }
                }
            );
        }
    });
}

/// A cell turning, scaling, fading and changing color forever, each at its own pace; all read
/// while drawing.
#[composable]
fn Cell(index: u32) {
    let transition = remember_infinite_transition("Cell");
    let rotation = transition.animate_float(
        0.0,
        360.0,
        infinite_repeatable(
            tween(1500 + index % 5 * 250).easing(LINEAR_EASING),
            RepeatMode::Restart,
        ),
    );
    let scale = transition.animate_float(
        0.6,
        1.0,
        infinite_repeatable(tween(700 + index % 7 * 100), RepeatMode::Reverse),
    );
    let alpha = transition.animate_float(
        0.35,
        1.0,
        infinite_repeatable(tween(900 + index % 3 * 300), RepeatMode::Reverse),
    );
    let fill = transition.animate_color(
        color(index),
        color(index + 3),
        infinite_repeatable(tween(1200 + index % 4 * 200), RepeatMode::Reverse),
    );
    Box!(
        modifier = Modifier
            .size(22.dp())
            .graphics_layer(move |layer| {
                layer.rotation_z = rotation.get();
                layer.scale_x = scale.get();
                layer.scale_y = scale.get();
                layer.alpha = alpha.get();
            })
            .draw_behind(move |scope| {
                let radius = scope.density().to_px(5.dp());
                scope.draw_round_rect(scope.bounds(), [radius; 4], Paint::fill(fill.get()));
            })
    );
}

#[composable]
fn Bars() {
    Column!(vertical_arrangement = Arrangement::spaced_by(6.dp()), {
        for bar in 0..BARS {
            key(bar, move || Bar(bar));
        }
    });
}

/// A bar whose width animates, read while measuring, beside its percentage, recomposed when it
/// changes.
#[composable]
fn Bar(index: u32) {
    let transition = remember_infinite_transition("Bar");
    let fraction = transition.animate_float(
        0.1,
        1.0,
        infinite_repeatable(tween(1100 + index * 170), RepeatMode::Reverse),
    );
    let fill = color(index);
    Row!(vertical_alignment = Alignment::CENTER_VERTICALLY, |row: RowScope| {
        Percent(fraction);
        Box!(modifier = Modifier.weight(row, 1.0).height(10.dp()), {
            Box!(
                modifier = Modifier
                    .layout(move |scope, measurable, constraints| {
                        let width = (constraints.max_width as f32 * fraction.get()).round() as i32;
                        let placeable = scope
                            .measure(measurable, Constraints::fixed(width, constraints.max_height));
                        scope.place(&placeable, 0, 0);
                        scope.layout(width, constraints.max_height)
                    })
                    .clip(RoundedCornerShape(5.dp()))
                    .background(fill)
            );
        });
    });
}

#[composable]
fn Percent(fraction: State<f32>) {
    let percent =
        remember(move || derived_state_of(move || (fraction.get() * 100.0).round() as i32));
    Text!(
        "{} %",
        percent.get(),
        modifier = Modifier.width(48.dp()),
        style = MaterialTheme::typography().label_large
    );
}

/// A card whose text grows to four lines and shrinks back to one every second, its size
/// animated.
#[composable]
fn ExpandingCard() {
    let expanded = remember(|| mutable_state_of(false));
    LaunchedEffect((), async move {
        loop {
            delay(Duration::from_millis(1000)).await;
            expanded.set(!expanded.get());
        }
    });
    let is_expanded = expanded.get();
    Card!(modifier = Modifier.fill_max_width(), {
        Column!(modifier = Modifier.animate_content_size_with(tween(500)).padding(12.dp()), {
            Text!("Release notes", style = MaterialTheme::typography().title_small);
            if is_expanded {
                Text!(
                    "{} {} {} {}",
                    SENTENCES[0],
                    SENTENCES[1],
                    SENTENCES[2],
                    SENTENCES[3],
                    style = MaterialTheme::typography().body_medium,
                    max_lines = 4,
                    overflow = TextOverflow::Ellipsis
                );
            } else {
                Text!(
                    "{}",
                    SENTENCES[0],
                    style = MaterialTheme::typography().body_medium,
                    max_lines = 1,
                    overflow = TextOverflow::Ellipsis
                );
            }
        });
    });
}

/// 50 sine waves, each with its own amplitude, frequency and speed, drawn anew each frame.
#[composable]
fn Waves() {
    let transition = remember_infinite_transition("Waves");
    let time = transition.animate_float(
        0.0,
        1.0,
        infinite_repeatable(tween(4000).easing(LINEAR_EASING), RepeatMode::Restart),
    );
    // One path, reset for each wave, as Stress.kt's: the recording keeps the contents of each.
    let path = remember(Path::new);
    Canvas(Modifier.fill_max_width().height(140.dp()), move |scope| {
        let time = time.get();
        let size = scope.size();
        let stroke = scope.density().to_px(1.5.dp());
        for line in 0..WAVES {
            let amplitude = size.height * 0.45 * (0.3 + 0.07 * (line % 10) as f32);
            let cycles = 1.0 + (line % 5) as f32 * 0.5;
            let phase = 2.0 * PI * time * (1 + line % 4) as f32 + line as f32 * 0.37;
            path.reset();
            for point in 0..=WAVE_SEGMENTS {
                let along = point as f32 / WAVE_SEGMENTS as f32;
                let x = size.width * along;
                let y = size.height / 2.0 + amplitude * (2.0 * PI * cycles * along + phase).sin();
                if point == 0 {
                    path.move_to(x, y);
                } else {
                    path.line_to(x, y);
                }
            }
            scope.draw_path(&path, Paint::stroke(color(line).with_alpha(0.6), stroke));
        }
    });
}
