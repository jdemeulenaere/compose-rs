//! `ListItem`, as Compose's Material 3 at androidx-main's tip: a row of a list, its content
//! (the headline) between optional leading and trailing content, an optional overline above and
//! supporting text below, on a shape and color that follow its state (selected, pressed,
//! focused, hovered, disabled). Compose's overloads are named apart, Rust having none (D181):
//! `ListItem` (not interactive), `ClickableListItem`, `SelectableListItem` and
//! `ToggleableListItem`.

use std::rc::Rc;

use compose_foundation::layout::Box;
use compose_runtime::{Callback, CompositionLocalProvider, composable};
use compose_ui::{
    Alignment, ClickableOptions, Color, Constraints, CornerSize, Dp, Layout, Measurable,
    MeasurePolicy, MeasureResult, MeasureScope, Modifier, MutableInteractionSource, PaddingValues,
    Placeable, Role, Shape, ToggleableState, UnitExt, VerticalAlignment,
};

use crate::components::{
    LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE, minimum_interactive_component_size,
};
use crate::theme::{LOCAL_CONTENT_COLOR, LOCAL_TEXT_STYLE, MaterialTheme};

/// A list item's colors in each state, as Compose's `ListItemColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ListItemColors {
    pub container_color: Color,
    pub content_color: Color,
    pub leading_content_color: Color,
    pub trailing_content_color: Color,
    pub overline_content_color: Color,
    pub supporting_content_color: Color,
    pub disabled_container_color: Color,
    pub disabled_content_color: Color,
    pub disabled_leading_content_color: Color,
    pub disabled_trailing_content_color: Color,
    pub disabled_overline_content_color: Color,
    pub disabled_supporting_content_color: Color,
    pub selected_container_color: Color,
    pub selected_content_color: Color,
    pub selected_leading_content_color: Color,
    pub selected_trailing_content_color: Color,
    pub selected_overline_content_color: Color,
    pub selected_supporting_content_color: Color,
    pub dragged_container_color: Color,
    pub dragged_content_color: Color,
    pub dragged_leading_content_color: Color,
    pub dragged_trailing_content_color: Color,
    pub dragged_overline_content_color: Color,
    pub dragged_supporting_content_color: Color,
}

impl compose_runtime::ComposeArg for ListItemColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// One of a list item's colors in a state: disabled, else dragged, else selected, else the
/// default, as `ListItemColors`' `containerColor(enabled, selected, dragged)` and its siblings.
fn pick(enabled: bool, selected: bool, dragged: bool, colors: [Color; 4]) -> Color {
    let [default, disabled, selected_color, dragged_color] = colors;
    if !enabled {
        disabled
    } else if dragged {
        dragged_color
    } else if selected {
        selected_color
    } else {
        default
    }
}

impl ListItemColors {
    pub fn container_color(&self, enabled: bool, selected: bool, dragged: bool) -> Color {
        pick(
            enabled,
            selected,
            dragged,
            [
                self.container_color,
                self.disabled_container_color,
                self.selected_container_color,
                self.dragged_container_color,
            ],
        )
    }

    pub fn content_color(&self, enabled: bool, selected: bool, dragged: bool) -> Color {
        pick(
            enabled,
            selected,
            dragged,
            [
                self.content_color,
                self.disabled_content_color,
                self.selected_content_color,
                self.dragged_content_color,
            ],
        )
    }

    pub fn leading_content_color(&self, enabled: bool, selected: bool, dragged: bool) -> Color {
        pick(
            enabled,
            selected,
            dragged,
            [
                self.leading_content_color,
                self.disabled_leading_content_color,
                self.selected_leading_content_color,
                self.dragged_leading_content_color,
            ],
        )
    }

    pub fn trailing_content_color(&self, enabled: bool, selected: bool, dragged: bool) -> Color {
        pick(
            enabled,
            selected,
            dragged,
            [
                self.trailing_content_color,
                self.disabled_trailing_content_color,
                self.selected_trailing_content_color,
                self.dragged_trailing_content_color,
            ],
        )
    }

    pub fn overline_content_color(&self, enabled: bool, selected: bool, dragged: bool) -> Color {
        pick(
            enabled,
            selected,
            dragged,
            [
                self.overline_content_color,
                self.disabled_overline_content_color,
                self.selected_overline_content_color,
                self.dragged_overline_content_color,
            ],
        )
    }

    pub fn supporting_content_color(&self, enabled: bool, selected: bool, dragged: bool) -> Color {
        pick(
            enabled,
            selected,
            dragged,
            [
                self.supporting_content_color,
                self.disabled_supporting_content_color,
                self.selected_supporting_content_color,
                self.dragged_supporting_content_color,
            ],
        )
    }
}

/// A list item's shape in each state, as Compose's `ListItemShapes`.
#[derive(Clone, PartialEq, Debug)]
pub struct ListItemShapes {
    pub shape: Shape,
    pub selected_shape: Shape,
    pub pressed_shape: Shape,
    pub focused_shape: Shape,
    pub hovered_shape: Shape,
    pub dragged_shape: Shape,
}

impl compose_runtime::ComposeArg for ListItemShapes {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl ListItemShapes {
    /// The shape for a state: pressed, then dragged, selected, focused, hovered, else the
    /// default, as Compose's `shapeForInteraction` (which also animates between them).
    fn for_state(&self, selected: bool, pressed: bool, focused: bool, hovered: bool) -> Shape {
        if pressed {
            self.pressed_shape.clone()
        } else if selected {
            self.selected_shape.clone()
        } else if focused {
            self.focused_shape.clone()
        } else if hovered {
            self.hovered_shape.clone()
        } else {
            self.shape.clone()
        }
    }
}

/// A list item's shadow elevation, at rest and dragged, as Compose's `ListItemElevation`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ListItemElevation {
    pub elevation: Dp,
    pub dragged_elevation: Dp,
}

impl compose_runtime::ComposeArg for ListItemElevation {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// As Compose's `ListItemDefaults`.
pub struct ListItemDefaults;

/// `ListTokens`: the item's paddings, the space between its parts, its heights for one, two and
/// three lines.
const ITEM_TOP_SPACE: Dp = Dp(10.0);
const ITEM_BOTTOM_SPACE: Dp = Dp(10.0);
const ITEM_LEADING_SPACE: Dp = Dp(16.0);
const ITEM_TRAILING_SPACE: Dp = Dp(16.0);
const ITEM_BETWEEN_SPACE: Dp = Dp(12.0);
const ONE_LINE_HEIGHT: Dp = Dp(56.0);
const TWO_LINE_HEIGHT: Dp = Dp(72.0);
const THREE_LINE_HEIGHT: Dp = Dp(88.0);
/// The content's height from which the default alignment is the top (below it, the center), as
/// `InteractiveListVerticalAlignmentBreakpoint`: half way between the two- and three-line
/// heights, less the vertical padding.
const VERTICAL_ALIGNMENT_BREAKPOINT: Dp =
    Dp((THREE_LINE_HEIGHT.0 + TWO_LINE_HEIGHT.0) / 2.0 - ITEM_TOP_SPACE.0 - ITEM_BOTTOM_SPACE.0);

impl ListItemDefaults {
    /// Around the item's parts, as `ContentPadding`: 16 dp at the sides, 10 dp at the top and
    /// bottom.
    pub const CONTENT_PADDING: PaddingValues = PaddingValues::new(
        ITEM_LEADING_SPACE,
        ITEM_TOP_SPACE,
        ITEM_TRAILING_SPACE,
        ITEM_BOTTOM_SPACE,
    );
    /// Between the items of a segmented list, as `SegmentedGap`.
    pub const SEGMENTED_GAP: Dp = Dp(2.0);

    /// The container's color, as `containerColor`: the surface.
    pub fn container_color() -> Color {
        MaterialTheme::color_scheme().surface
    }

    /// The headline's color, as `contentColor`: on the surface.
    pub fn content_color() -> Color {
        MaterialTheme::color_scheme().on_surface
    }

    /// From the theme, as `ListItemDefaults.colors()` (`ListTokens`, `ReorderListTokens`).
    pub fn colors() -> ListItemColors {
        let scheme = MaterialTheme::color_scheme();
        let disabled = scheme.on_surface.with_alpha(0.38);
        ListItemColors {
            container_color: scheme.surface,
            content_color: scheme.on_surface,
            leading_content_color: scheme.on_surface_variant,
            trailing_content_color: scheme.on_surface_variant,
            overline_content_color: scheme.on_surface_variant,
            supporting_content_color: scheme.on_surface_variant,
            disabled_container_color: scheme.surface,
            disabled_content_color: disabled,
            disabled_leading_content_color: disabled,
            disabled_trailing_content_color: disabled,
            disabled_overline_content_color: disabled,
            disabled_supporting_content_color: disabled,
            selected_container_color: scheme.secondary_container,
            selected_content_color: scheme.on_secondary_container,
            selected_leading_content_color: scheme.on_secondary_container,
            selected_trailing_content_color: scheme.on_secondary_container,
            selected_overline_content_color: scheme.on_secondary_container,
            selected_supporting_content_color: scheme.on_secondary_container,
            dragged_container_color: scheme.tertiary_container,
            dragged_content_color: scheme.on_tertiary_container,
            dragged_leading_content_color: scheme.on_tertiary_container,
            dragged_trailing_content_color: scheme.on_tertiary_container,
            dragged_overline_content_color: scheme.on_tertiary_container,
            dragged_supporting_content_color: scheme.on_tertiary_container,
        }
    }

    /// The items of a segmented list's colors, as `segmentedColors()` (the same roles here: the
    /// segmented container is the surface too).
    pub fn segmented_colors() -> ListItemColors {
        ListItemDefaults::colors()
    }

    /// From the theme's shapes, as `ListItemDefaults.shapes()`: extra small at rest, large
    /// selected, pressed, focused or dragged, medium hovered.
    pub fn shapes() -> ListItemShapes {
        let shapes = MaterialTheme::shapes();
        ListItemShapes {
            shape: shapes.extra_small.clone(),
            selected_shape: shapes.large.clone(),
            pressed_shape: shapes.large.clone(),
            focused_shape: shapes.large.clone(),
            hovered_shape: shapes.medium.clone(),
            dragged_shape: shapes.large.clone(),
        }
    }

    /// No shadow at rest, 8 dp dragged (`ElevationTokens.Level4`), as `elevation()`.
    pub fn elevation() -> ListItemElevation {
        ListItemElevation { elevation: Dp(0.0), dragged_elevation: Dp(8.0) }
    }

    /// The shapes of the item at `index` of `count` in a segmented list, as `segmentedShapes`:
    /// the first's top corners and the last's bottom corners large (`ListTokens.ContainerShape`),
    /// both for a single item.
    pub fn segmented_shapes(
        index: usize,
        count: usize,
        default_shapes: ListItemShapes,
    ) -> ListItemShapes {
        let (Shape::Rounded(corners), Shape::Rounded(large)) =
            (default_shapes.shape.clone(), MaterialTheme::shapes().large.clone())
        else {
            return default_shapes;
        };
        // Top start, top end, bottom end, bottom start.
        let mut corners: [CornerSize; 4] = corners;
        if index == 0 {
            corners[0] = large[0];
            corners[1] = large[1];
        }
        if index + 1 == count {
            corners[2] = large[2];
            corners[3] = large[3];
        }
        ListItemShapes { shape: Shape::Rounded(corners), ..default_shapes }
    }
}

/// What makes an item interactive.
struct ItemClick {
    on_click: Callback,
    on_long_click: Option<Callback>,
    interaction_source: MutableInteractionSource,
}

/// The item's role and state in semantics.
#[derive(Clone, Copy)]
enum ItemSemantics {
    None,
    Selectable(bool),
    Toggleable(bool),
}

/// A list item, as Compose's `ListItem(…, content)`: not interactive. `content` is the
/// headline (body large), `overline_content` above it (label small), `supporting_content`
/// below (body medium), `leading_content` and `trailing_content` at its sides, 12 dp from the
/// rest; inside `content_padding`, at least 56, 72 or 88 dp high for one, two or three lines.
/// `vertical_alignment` aligns the parts: `None` is Compose's default
/// (`ListItemDefaults.verticalAlignment()`: centered when the space is under 60 dp, else at the
/// top). One semantics node, its texts merged.
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn ListItem(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] leading_content: Option<impl Fn() + 'static>,
    #[default(None)] trailing_content: Option<impl Fn() + 'static>,
    #[default(None)] overline_content: Option<impl Fn() + 'static>,
    #[default(None)] supporting_content: Option<impl Fn() + 'static>,
    #[default(None)] vertical_alignment: Option<VerticalAlignment>,
    #[default(crate::ListItemDefaults::shapes())] shapes: ListItemShapes,
    #[default(crate::ListItemDefaults::colors())] colors: ListItemColors,
    #[default(crate::ListItemDefaults::elevation())] elevation: ListItemElevation,
    #[default(crate::ListItemDefaults::CONTENT_PADDING)] content_padding: PaddingValues,
    content: impl Fn() + 'static,
) {
    let slots =
        Slots { leading_content, trailing_content, overline_content, supporting_content, content };
    interactive_list_item(
        modifier,
        slots,
        vertical_alignment,
        enabled,
        false,
        ItemSemantics::None,
        None,
        colors,
        shapes,
        elevation,
        content_padding,
    );
}

/// A clickable list item, as Compose's `ListItem(onClick, …)`: at least 48 dp for touch, a
/// state layer while pressed, focused or hovered, `on_long_click` on a long press.
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn ClickableListItem(
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] leading_content: Option<impl Fn() + 'static>,
    #[default(None)] trailing_content: Option<impl Fn() + 'static>,
    #[default(None)] overline_content: Option<impl Fn() + 'static>,
    #[default(None)] supporting_content: Option<impl Fn() + 'static>,
    #[default(None)] vertical_alignment: Option<VerticalAlignment>,
    #[default(None)] on_long_click: Option<impl Fn() + 'static>,
    #[default(crate::ListItemDefaults::shapes())] shapes: ListItemShapes,
    #[default(crate::ListItemDefaults::colors())] colors: ListItemColors,
    #[default(crate::ListItemDefaults::elevation())] elevation: ListItemElevation,
    #[default(crate::ListItemDefaults::CONTENT_PADDING)] content_padding: PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn() + 'static,
) {
    let interaction_source = crate::components::remembered_interaction_source(interaction_source);
    let slots =
        Slots { leading_content, trailing_content, overline_content, supporting_content, content };
    interactive_list_item(
        modifier,
        slots,
        vertical_alignment,
        enabled,
        false,
        ItemSemantics::None,
        Some(ItemClick { on_click, on_long_click, interaction_source }),
        colors,
        shapes,
        elevation,
        content_padding,
    );
}

/// A selectable list item, as Compose's `ListItem(selected, onClick, …)`: a clickable item
/// whose semantics say whether it's `selected` (a radio button's role), in the selected colors
/// and shape when it is.
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn SelectableListItem(
    selected: bool,
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] leading_content: Option<impl Fn() + 'static>,
    #[default(None)] trailing_content: Option<impl Fn() + 'static>,
    #[default(None)] overline_content: Option<impl Fn() + 'static>,
    #[default(None)] supporting_content: Option<impl Fn() + 'static>,
    #[default(None)] vertical_alignment: Option<VerticalAlignment>,
    #[default(None)] on_long_click: Option<impl Fn() + 'static>,
    #[default(crate::ListItemDefaults::shapes())] shapes: ListItemShapes,
    #[default(crate::ListItemDefaults::colors())] colors: ListItemColors,
    #[default(crate::ListItemDefaults::elevation())] elevation: ListItemElevation,
    #[default(crate::ListItemDefaults::CONTENT_PADDING)] content_padding: PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn() + 'static,
) {
    let interaction_source = crate::components::remembered_interaction_source(interaction_source);
    let slots =
        Slots { leading_content, trailing_content, overline_content, supporting_content, content };
    interactive_list_item(
        modifier,
        slots,
        vertical_alignment,
        enabled,
        selected,
        ItemSemantics::Selectable(selected),
        Some(ItemClick { on_click, on_long_click, interaction_source }),
        colors,
        shapes,
        elevation,
        content_padding,
    );
}

/// A toggleable list item, as Compose's `ListItem(checked, onCheckedChange, …)`: a clickable
/// item whose click calls `on_checked_change` with the opposite of `checked`, its semantics a
/// checkbox's on or off, in the selected colors and shape while checked.
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn ToggleableListItem(
    checked: bool,
    on_checked_change: impl Fn(bool) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enabled: bool,
    #[default(None)] leading_content: Option<impl Fn() + 'static>,
    #[default(None)] trailing_content: Option<impl Fn() + 'static>,
    #[default(None)] overline_content: Option<impl Fn() + 'static>,
    #[default(None)] supporting_content: Option<impl Fn() + 'static>,
    #[default(None)] vertical_alignment: Option<VerticalAlignment>,
    #[default(None)] on_long_click: Option<impl Fn() + 'static>,
    #[default(crate::ListItemDefaults::shapes())] shapes: ListItemShapes,
    #[default(crate::ListItemDefaults::colors())] colors: ListItemColors,
    #[default(crate::ListItemDefaults::elevation())] elevation: ListItemElevation,
    #[default(crate::ListItemDefaults::CONTENT_PADDING)] content_padding: PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
    content: impl Fn() + 'static,
) {
    let interaction_source = crate::components::remembered_interaction_source(interaction_source);
    let slots =
        Slots { leading_content, trailing_content, overline_content, supporting_content, content };
    let on_click = compose_runtime::callback(move || on_checked_change(!checked));
    interactive_list_item(
        modifier,
        slots,
        vertical_alignment,
        enabled,
        checked,
        ItemSemantics::Toggleable(checked),
        Some(ItemClick { on_click, on_long_click, interaction_source }),
        colors,
        shapes,
        elevation,
        content_padding,
    );
}

/// An item's contents.
struct Slots {
    leading_content: Option<Callback>,
    trailing_content: Option<Callback>,
    overline_content: Option<Callback>,
    supporting_content: Option<Callback>,
    content: Callback,
}

/// The body the list items share, as Compose's `InteractiveListItem`: its colors and shape for
/// its state (snapped: Compose animates them, with its motion scheme), its shadow, its click,
/// and the layout of its parts.
#[allow(clippy::too_many_arguments)]
fn interactive_list_item(
    modifier: Modifier,
    slots: Slots,
    vertical_alignment: Option<VerticalAlignment>,
    enabled: bool,
    selected: bool,
    semantics: ItemSemantics,
    click: Option<ItemClick>,
    colors: ListItemColors,
    shapes: ListItemShapes,
    elevation: ListItemElevation,
    content_padding: PaddingValues,
) {
    let (pressed, focused, hovered) = match &click {
        Some(click) => (
            click.interaction_source.is_pressed(),
            click.interaction_source.is_focused(),
            click.interaction_source.is_hovered(),
        ),
        None => (false, false, false),
    };
    // compose-rs has no drag interactions: an item is never dragged here.
    let dragged = false;
    let shape = shapes.for_state(selected, pressed, focused, hovered);
    let container = colors.container_color(enabled, selected, dragged);
    let content_color = colors.content_color(enabled, selected, dragged);
    let shadow = if dragged { elevation.dragged_elevation } else { elevation.elevation };
    let is_clickable = click.is_some();
    let modifier = modifier.semantics_with(true, move |config| match semantics {
        ItemSemantics::None => {
            if !enabled && !is_clickable {
                config.disabled();
            }
        }
        ItemSemantics::Selectable(selected) => {
            config.set_selected(selected);
            config.set_role(Role::RadioButton);
        }
        ItemSemantics::Toggleable(checked) => {
            config.set_toggleable_state(if checked {
                ToggleableState::On
            } else {
                ToggleableState::Off
            });
            config.set_role(Role::Checkbox);
        }
    });
    let modifier =
        if is_clickable { minimum_interactive_component_size(modifier) } else { modifier };
    // Above its neighbours while it casts a shadow, as Compose's `zIndexLambda`.
    let modifier = if shadow.0 > 0.0 { modifier.z_index(1.0) } else { modifier };
    let modifier = modifier
        .shadow_with(
            shadow,
            shape.clone(),
            false,
            compose_ui::DEFAULT_SHADOW_COLOR,
            compose_ui::DEFAULT_SHADOW_COLOR,
        )
        .background_with(container, shape.clone())
        .clip(shape);
    let modifier = match click {
        Some(click) => modifier.combined_clickable_with(
            ClickableOptions {
                enabled,
                indication: content_color.into(),
                interaction_source: Some(click.interaction_source),
                ..ClickableOptions::DEFAULT
            },
            click.on_long_click,
            None,
            click.on_click,
        ),
        None => modifier,
    };
    let modifier = modifier.padding(content_padding);
    let typography = MaterialTheme::typography();
    let leading_color = colors.leading_content_color(enabled, selected, dragged);
    let trailing_color = colors.trailing_content_color(enabled, selected, dragged);
    let overline_color = colors.overline_content_color(enabled, selected, dragged);
    let supporting_color = colors.supporting_content_color(enabled, selected, dragged);
    let vertical_padding = content_padding.top + content_padding.bottom;
    let policy = compose_runtime::remember_keyed(
        PolicyKey(
            vertical_alignment.map(|alignment| alignment.bias),
            vertical_padding,
            [
                slots.leading_content.is_some(),
                slots.trailing_content.is_some(),
                slots.overline_content.is_some(),
                slots.supporting_content.is_some(),
            ],
        ),
        || {
            Rc::new(ListItemLayout {
                vertical_alignment,
                vertical_padding,
                leading: slots.leading_content.is_some(),
                trailing: slots.trailing_content.is_some(),
                overline: slots.overline_content.is_some(),
                supporting: slots.supporting_content.is_some(),
            }) as Rc<dyn MeasurePolicy>
        },
    );
    let Slots { leading_content, trailing_content, overline_content, supporting_content, content } =
        slots;
    let (leading_start, trailing_end) = (content_padding.start, content_padding.end);
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(content_color), || {
        Layout(
            modifier.clone(),
            policy.clone(),
            compose_runtime::callback(move || {
                if let Some(leading) = leading_content.clone() {
                    side_decorator(
                        (0.dp(), 0.dp(), ITEM_BETWEEN_SPACE, 0.dp()),
                        leading_start + ITEM_BETWEEN_SPACE,
                        leading_color,
                        typography.title_medium.clone(),
                        leading,
                    );
                }
                if let Some(trailing) = trailing_content.clone() {
                    side_decorator(
                        (ITEM_BETWEEN_SPACE, 0.dp(), 0.dp(), 0.dp()),
                        trailing_end + ITEM_BETWEEN_SPACE,
                        trailing_color,
                        typography.label_small.clone(),
                        trailing,
                    );
                }
                if let Some(overline) = overline_content.clone() {
                    decorator(overline_color, typography.label_small.clone(), overline);
                }
                decorator(content_color, typography.body_large.clone(), content.clone());
                if let Some(supporting) = supporting_content.clone() {
                    decorator(supporting_color, typography.body_medium.clone(), supporting);
                }
            }),
        );
    });
}

#[derive(Clone, Copy, PartialEq)]
struct PolicyKey(Option<f32>, Dp, [bool; 4]);

impl compose_runtime::ComposeArg for PolicyKey {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A part of the item in a box, in `color` and `style`, as Compose's `ContentDecorator` and its
/// siblings.
fn decorator(color: Color, style: compose_ui::TextStyle, content: Callback) {
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), || {
        CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(style), || {
            let content = content.clone();
            Box(
                Modifier,
                Alignment::TOP_START,
                false,
                compose_runtime::callback(move |_| content()),
            );
        });
    });
}

/// The leading or trailing content: `padding` away from the rest, and its minimum interactive
/// size less the space around it (`around`: the item's padding and the 12 dp to the rest),
/// which counts towards its touch target, as Compose's `LeadingDecorator` and
/// `TrailingDecorator`.
fn side_decorator(
    padding: (Dp, Dp, Dp, Dp),
    around: Dp,
    color: Color,
    style: compose_ui::TextStyle,
    content: Callback,
) {
    let minimum = LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE.current();
    let minimum = if minimum.is_specified() { minimum } else { Dp(0.0) };
    let reduced = Dp((minimum.0 - around.0).max(0.0));
    CompositionLocalProvider(LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE.provides(reduced), || {
        CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), || {
            CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(style), || {
                let content = content.clone();
                Box(
                    Modifier.padding(padding),
                    Alignment::TOP_START,
                    false,
                    compose_runtime::callback(move |_| content()),
                );
            });
        });
    });
}

/// Lays an item's parts out as Compose's `InteractiveListItemMeasurePolicy`: the leading and
/// trailing content at their natural sizes, the overline, the content and the supporting text
/// stacked in the width left, as wide as allowed (or as its parts when unbounded); at least the
/// one-, two- or three-line height less the vertical padding (unless the item has a minimum
/// height of its own); each side and the stack aligned vertically.
struct ListItemLayout {
    vertical_alignment: Option<VerticalAlignment>,
    vertical_padding: Dp,
    leading: bool,
    trailing: bool,
    overline: bool,
    supporting: bool,
}

/// A supporting text taller than this is taken for two lines (Compose compares its first and
/// last baselines; compose-rs has no alignment lines, so it uses Compose's intrinsic
/// heuristic, `isSupportingMultilineHeuristic`).
const MULTILINE_SUPPORTING_HEIGHT: compose_ui::TextUnit = compose_ui::TextUnit::sp(30.0);

impl MeasurePolicy for ListItemLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let mut children = measurables.iter().copied();
        let mut next = |present: bool| if present { children.next() } else { None };
        let (leading, trailing, overline) =
            (next(self.leading), next(self.trailing), next(self.overline));
        let content = next(true);
        let supporting = next(self.supporting);
        let loose = Constraints::new(0, constraints.max_width, 0, constraints.max_height);
        let mut offset_x = 0;
        let mut offset_y = 0;
        let mut measure = |child: Option<Measurable>, x: i32, y: i32| -> Option<Placeable> {
            child.map(|child| scope.measure(child, loose.offset(-x, -y)))
        };
        let leading = measure(leading, 0, 0);
        offset_x += width_or_zero(&leading);
        let trailing = measure(trailing, offset_x, 0);
        offset_x += width_or_zero(&trailing);
        let overline = measure(overline, offset_x, 0);
        offset_y += height_or_zero(&overline);
        let content = measure(content, offset_x, offset_y);
        offset_y += height_or_zero(&content);
        let supporting = measure(supporting, offset_x, offset_y);
        let density = scope.density();
        let multiline = supporting.as_ref().is_some_and(|it| {
            it.height as f32 > density.sp_to_px(MULTILINE_SUPPORTING_HEIGHT).round()
        });
        let lines = if (overline.is_some() && supporting.is_some()) || multiline {
            3
        } else if overline.is_some() || supporting.is_some() {
            2
        } else {
            1
        };
        let width = if constraints.has_bounded_width() {
            constraints.max_width
        } else {
            width_or_zero(&leading)
                + width_or_zero(&content)
                    .max(width_or_zero(&overline))
                    .max(width_or_zero(&supporting))
                + width_or_zero(&trailing)
        };
        let stack =
            height_or_zero(&content) + height_or_zero(&overline) + height_or_zero(&supporting);
        let natural = height_or_zero(&leading).max(stack).max(height_or_zero(&trailing));
        let default_min = match lines {
            1 => ONE_LINE_HEIGHT,
            2 => TWO_LINE_HEIGHT,
            _ => THREE_LINE_HEIGHT,
        };
        let min_height = if constraints.min_height == 0 {
            density
                .round_to_px(Dp((default_min.0 - self.vertical_padding.0).max(0.0)))
                .clamp(0, constraints.max_height)
        } else {
            constraints.min_height
        };
        let height = natural.clamp(min_height, constraints.max_height.max(min_height));
        let align = |size: i32| match self.vertical_alignment {
            Some(alignment) => alignment.align(size, height),
            None => {
                let breakpoint = density.round_to_px(VERTICAL_ALIGNMENT_BREAKPOINT);
                let base =
                    if height < breakpoint { Alignment::CENTER_VERTICALLY } else { Alignment::TOP };
                base.align(size, height)
            }
        };
        if let Some(leading) = &leading {
            scope.place_relative(leading, 0, align(leading.height));
        }
        let x = width_or_zero(&leading);
        let mut y = align(stack);
        for part in [&overline, &content, &supporting].into_iter().flatten() {
            scope.place_relative(part, x, y);
            y += part.height;
        }
        if let Some(trailing) = &trailing {
            scope.place_relative(trailing, width - trailing.width, align(trailing.height));
        }
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "ListItem"
    }
}

fn width_or_zero(placeable: &Option<Placeable>) -> i32 {
    placeable.as_ref().map_or(0, |it| it.width)
}

fn height_or_zero(placeable: &Option<Placeable>) -> i32 {
    placeable.as_ref().map_or(0, |it| it.height)
}
