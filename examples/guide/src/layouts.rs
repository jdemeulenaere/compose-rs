//! Layouts (docs/guide/kotlin-to-rust.md, "Layouts").

use compose::foundation::layout::*;
use compose::foundation::*;
use compose::material3::*;
use compose::runtime::*;
use compose::ui::*;

// ANCHOR: row_column_box
#[composable]
pub fn Author(name: SharedString, online: bool) {
    Row!(
        modifier = Modifier.fill_max_width().padding((16.dp(), 8.dp())),
        horizontal_arrangement = Arrangement::spaced_by(12.dp()),
        vertical_alignment = Alignment::CENTER_VERTICALLY,
        |row| {
            Box!(modifier = Modifier.size(40.dp()), content_alignment = Alignment::BOTTOM_END, {
                Box!(
                    modifier = Modifier
                        .fill_max_size()
                        .background_with(MaterialTheme::color_scheme().primary, CIRCLE_SHAPE)
                );
                if online {
                    Box!(
                        modifier =
                            Modifier.size(10.dp()).background_with(Color::GREEN, CIRCLE_SHAPE)
                    );
                }
            });
            // Receiver-scoped modifiers take the scope: Kotlin's `Modifier.weight(1f)` in a `RowScope`.
            Text!(name, modifier = Modifier.weight(row, 1.0));
            TextButton!(on_click = || {}, {
                Text!("Follow");
            });
        }
    );
}
// ANCHOR_END: row_column_box

// ANCHOR: modifiers
#[composable]
pub fn ModifierForms(selected: bool, on_click: impl Fn() + 'static) {
    let shape = RoundedCornerShape(8.dp());
    Column!({
        // One method per Compose name; a tuple for an overload taking several values.
        Box!(modifier = Modifier.padding(8.dp())); // padding(all)
        Box!(modifier = Modifier.padding((16.dp(), 8.dp()))); // padding(horizontal, vertical)
        Box!(modifier = Modifier.padding((1.dp(), 2.dp(), 3.dp(), 4.dp()))); // start, top, end, bottom
        Box!(modifier = Modifier.size(48.dp()).size((48.dp(), 24.dp()))); // size(size), size(width, height)
        // Compose's optional parameters: the short form takes their defaults, `_with` takes them all.
        Box!(modifier = Modifier.fill_max_width_with(0.5)); // fillMaxWidth(fraction = 0.5f)
        Box!(modifier = Modifier.background(Color::RED)); // background(color)
        Box!(modifier = Modifier.background_with(Color::RED, shape)); // background(color, shape)
        Box!(modifier = Modifier.border_with(1.dp(), Color::BLACK, shape));
        // The clickable family's optional parameters are a struct.
        Box!(
            modifier = Modifier.size(48.dp()).clickable_with(
                ClickableOptions {
                    enabled: !selected,
                    role: Some(Role::Button),
                    ..ClickableOptions::DEFAULT
                },
                on_click,
            )
        );
    });
}
// ANCHOR_END: modifiers

// ANCHOR: arrangements
#[composable]
pub fn Toolbar(compact: bool) {
    Row!(
        modifier = Modifier.fill_max_width(),
        // Compose's `Arrangement.SpaceBetween`, in Rust's constant case.
        horizontal_arrangement = Arrangement::SPACE_BETWEEN,
        vertical_alignment = Alignment::CENTER_VERTICALLY,
        {
            Text!("Inbox");
            Row!(
                // `END` is horizontal only, `spaced_by` either way: `.into()` makes them one type.
                horizontal_arrangement =
                    if compact { Arrangement::END } else { Arrangement::spaced_by(8.dp()).into() },
                {
                    Text!("Edit");
                    Text!("Share");
                }
            );
        }
    );
    Column!(
        vertical_arrangement = Arrangement::spaced_by((4.dp(), Alignment::BOTTOM)),
        horizontal_alignment = Alignment::CENTER_HORIZONTALLY,
        {
            Text!("One");
            Text!("Two");
        }
    );
}
// ANCHOR_END: arrangements

// ANCHOR: lazy
#[derive(Clone, PartialEq)]
pub struct Message {
    pub id: u64,
    pub text: SharedString,
}

#[derive(Clone, PartialEq)]
pub struct Day {
    pub title: SharedString,
    pub messages: ImmutableList<Message>,
}

#[composable]
pub fn Messages(days: ImmutableList<Day>, state: LazyListState) {
    LazyColumn!(
        modifier = Modifier.fill_max_size(),
        state = state,
        content_padding = PaddingValues::from(8.dp()),
        vertical_arrangement = Arrangement::spaced_by(4.dp()),
        |scope: LazyListScope| {
            for day in days.iter() {
                let title = day.title.clone();
                scope.sticky_header_keyed(title.clone(), move |_item| {
                    Text!(
                        title.clone(),
                        modifier = Modifier
                            .fill_max_width()
                            .background(MaterialTheme::color_scheme().surface)
                    );
                });
                // Keys keep an item's state (and the scroll position) when items move.
                scope.items_list_keyed(
                    day.messages.clone(),
                    |message| message.id,
                    |_item, message| {
                        Text!(&message.text);
                    },
                );
            }
        }
    );
}
// ANCHOR_END: lazy

// ANCHOR: custom_layout
/// Its children left to right, wrapping to a new line when one doesn't fit: a minimal FlowRow.
#[composable]
pub fn Wrap(#[default(Modifier)] modifier: Modifier, content: impl Fn() + 'static) {
    // Remembered: a new policy each pass would be a changed argument, so `Layout` couldn't skip.
    let policy = remember(|| {
        measure_policy(|scope, measurables, constraints| {
            let (mut x, mut y, mut line_height, mut width) = (0, 0, 0, 0);
            for &measurable in measurables {
                let placeable = scope.measure(measurable, constraints.loosen());
                if x > 0 && x + placeable.width > constraints.max_width {
                    (x, y, line_height) = (0, y + line_height, 0);
                }
                scope.place(&placeable, x, y);
                x += placeable.width;
                width = width.max(x);
                line_height = line_height.max(placeable.height);
            }
            scope.layout(width, y + line_height)
        })
    });
    Layout!(modifier = modifier, measure_policy = policy, content = content);
}
// ANCHOR_END: custom_layout

// ANCHOR: layout_modifier
/// `Modifier.indent(level)`, as a Kotlin extension function: a trait on `Modifier`.
pub trait IndentModifier {
    fn indent(self, level: u32) -> Modifier;
}

impl IndentModifier for Modifier {
    fn indent(self, level: u32) -> Modifier {
        self.layout(move |scope, measurable, constraints| {
            let indent = scope.density().round_to_px(16.dp()) * level as i32;
            let inner =
                Constraints { max_width: (constraints.max_width - indent).max(0), ..constraints };
            let placeable = scope.measure(measurable, inner.loosen());
            scope.place(&placeable, indent, 0);
            scope.layout(placeable.width + indent, placeable.height)
        })
    }
}
// ANCHOR_END: layout_modifier

#[composable]
pub fn Outline() {
    Column!({
        Text!("Chapter", modifier = Modifier.indent(0));
        Text!("Section", modifier = Modifier.indent(1).test_tag("section"));
    });
}
