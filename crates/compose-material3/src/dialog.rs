//! `AlertDialog`, `BasicAlertDialog`, `DropdownMenu` and `DropdownMenuItem`, as Compose's
//! Material 3.

use std::cell::Cell;
use std::rc::Rc;

use compose_animation::{AnimationSpec, MutableTransitionState, remember_transition};
use compose_foundation::BorderStroke;
use compose_foundation::ScrollModifiers as _;
use compose_foundation::layout::{Arrangement, Box, Column, ColumnScope};
use compose_runtime::{CompositionLocalProvider, composable, remember};
use compose_ui::{
    Alignment, ClickableOptions, Color, Constraints, Dp, DpOffset, IntOffset, IntSize, Layout,
    LayoutDirection, Measurable, MeasurePolicy, MeasureResult, MeasureScope, Modifier,
    MutableInteractionSource, PaddingValues, Placeable, PopupPositionProvider, PopupProperties,
    Rect, Shape, TransformOrigin, UnitExt,
};

use crate::components::Surface;
use crate::theme::{LOCAL_CONTENT_COLOR, LOCAL_TEXT_STYLE, MaterialTheme};

// ---- AlertDialog -----------------------------------------------------------------------------

/// As Compose's `AlertDialogDefaults`.
pub struct AlertDialogDefaults;

impl AlertDialogDefaults {
    /// The dialog's icon, as `IconSize`.
    pub const ICON_SIZE: Dp = Dp(24.0);
    /// No tonal elevation, as `TonalElevation`.
    pub const TONAL_ELEVATION: Dp = Dp(0.0);

    /// The extra-large shape, as `shape` (`DialogTokens.ContainerShape`).
    pub fn shape() -> compose_ui::Shape {
        MaterialTheme::shapes().extra_large.clone()
    }

    /// The high surface container, as `containerColor`.
    pub fn container_color() -> compose_ui::Color {
        MaterialTheme::color_scheme().surface_container_high
    }

    /// The secondary color, as `iconContentColor`.
    pub fn icon_content_color() -> compose_ui::Color {
        MaterialTheme::color_scheme().secondary
    }

    /// On-surface, as `titleContentColor`.
    pub fn title_content_color() -> compose_ui::Color {
        MaterialTheme::color_scheme().on_surface
    }

    /// On-surface-variant, as `textContentColor`.
    pub fn text_content_color() -> compose_ui::Color {
        MaterialTheme::color_scheme().on_surface_variant
    }
}

/// A dialog's width bounds, as `DialogMinWidth` and `DialogMaxWidth`.
const DIALOG_MIN_WIDTH: Dp = Dp(280.0);
const DIALOG_MAX_WIDTH: Dp = Dp(560.0);
/// Between the buttons, along and across their rows, as `ButtonsMainAxisSpacing` and
/// `ButtonsCrossAxisSpacing`.
const BUTTONS_MAIN_AXIS_SPACING: Dp = Dp(8.0);
const BUTTONS_CROSS_AXIS_SPACING: Dp = Dp(8.0);

/// A dialog of any content, as Compose's `BasicAlertDialog`: `content` 280 to 560 dp wide (its
/// minimum passed on to the content), in a dialog dismissed by `on_dismiss_request`. The content
/// styles itself.
#[composable]
#[allow(non_snake_case)]
pub fn BasicAlertDialog(
    on_dismiss_request: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::window_default_dialog_properties())] properties: compose_ui::DialogProperties,
    content: impl Fn() + 'static,
) {
    compose_ui::Dialog(
        on_dismiss_request,
        properties,
        compose_runtime::callback(move || {
            let content = content.clone();
            Box(
                modifier.clone().size_in(compose_ui::SizeSpec::bounds(
                    DIALOG_MIN_WIDTH,
                    Dp::UNSPECIFIED,
                    DIALOG_MAX_WIDTH,
                    Dp::UNSPECIFIED,
                )),
                Alignment::TOP_START,
                true,
                compose_runtime::callback(move |_| content()),
            );
        }),
    );
}

/// A dialog with a title, text and buttons, as Compose's `AlertDialog`: a `BasicAlertDialog`
/// of a surface (`shape`, `container_color`, `tonal_elevation`), 24 dp in; the icon (in
/// `icon_content_color`) and the title (in `title_content_color`, the headline small style)
/// centered above the text (in `text_content_color`, body medium), which takes the room left;
/// the buttons at the end (in the primary color, label large), 8 dp apart, the confirm button
/// last, or first when they're stacked because they don't fit on one row.
#[composable]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn AlertDialog(
    on_dismiss_request: impl Fn() + 'static,
    confirm_button: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] dismiss_button: Option<impl Fn() + 'static>,
    #[default(None)] icon: Option<impl Fn() + 'static>,
    #[default(None)] title: Option<impl Fn() + 'static>,
    #[default(None)] text: Option<impl Fn() + 'static>,
    #[default(crate::AlertDialogDefaults::shape())] shape: compose_ui::Shape,
    #[default(crate::AlertDialogDefaults::container_color())] container_color: compose_ui::Color,
    #[default(crate::AlertDialogDefaults::icon_content_color())]
    icon_content_color: compose_ui::Color,
    #[default(crate::AlertDialogDefaults::title_content_color())]
    title_content_color: compose_ui::Color,
    #[default(crate::AlertDialogDefaults::text_content_color())]
    text_content_color: compose_ui::Color,
    #[default(crate::AlertDialogDefaults::TONAL_ELEVATION)] tonal_elevation: Dp,
    #[default(crate::window_default_dialog_properties())] properties: compose_ui::DialogProperties,
) {
    let content = AlertDialogParts {
        confirm_button,
        dismiss_button,
        icon,
        title,
        text,
        shape,
        container_color,
        icon_content_color,
        title_content_color,
        text_content_color,
        tonal_elevation,
    };
    BasicAlertDialog(
        on_dismiss_request,
        modifier,
        properties,
        compose_runtime::callback(move || alert_dialog_content(&content)),
    );
}

/// What `AlertDialog` lays out in its dialog.
#[derive(Clone)]
struct AlertDialogParts {
    confirm_button: compose_runtime::Callback,
    dismiss_button: Option<compose_runtime::Callback>,
    icon: Option<compose_runtime::Callback>,
    title: Option<compose_runtime::Callback>,
    text: Option<compose_runtime::Callback>,
    shape: compose_ui::Shape,
    container_color: compose_ui::Color,
    icon_content_color: compose_ui::Color,
    title_content_color: compose_ui::Color,
    text_content_color: compose_ui::Color,
    tonal_elevation: Dp,
}

/// `content` in `color` and `style` merged into the text style, as Compose's
/// `ProvideContentColorTextStyle`.
fn provide_content_color_text_style(
    color: compose_ui::Color,
    style: compose_ui::TextStyle,
    content: impl FnOnce(),
) {
    let merged = LOCAL_TEXT_STYLE.current().merge(&style);
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), || {
        CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(merged), content);
    });
}

/// As Compose's `AlertDialogContent`.
fn alert_dialog_content(parts: &AlertDialogParts) {
    let parts = parts.clone();
    Surface(
        Modifier,
        parts.shape.clone(),
        parts.container_color,
        compose_ui::Color::UNSPECIFIED,
        parts.tonal_elevation,
        Dp(0.0),
        None,
        compose_runtime::callback(move || {
            let parts = parts.clone();
            Column(
                Modifier.padding(24.dp()),
                Arrangement::TOP,
                compose_ui::Alignment::START,
                compose_runtime::callback(move |column: ColumnScope| {
                    let typography = MaterialTheme::typography();
                    if let Some(icon) = parts.icon.clone() {
                        CompositionLocalProvider(
                            LOCAL_CONTENT_COLOR.provides(parts.icon_content_color),
                            || {
                                Box(
                                    Modifier
                                        .padding((0.dp(), 0.dp(), 0.dp(), 16.dp()))
                                        .align(column, compose_ui::Alignment::CENTER_HORIZONTALLY),
                                    Alignment::TOP_START,
                                    false,
                                    compose_runtime::callback(move |_| icon()),
                                );
                            },
                        );
                    }
                    if let Some(title) = parts.title.clone() {
                        // Centered under an icon.
                        let alignment = if parts.icon.is_some() {
                            compose_ui::Alignment::CENTER_HORIZONTALLY
                        } else {
                            compose_ui::Alignment::START
                        };
                        provide_content_color_text_style(
                            parts.title_content_color,
                            typography.headline_small.clone(),
                            || {
                                Box(
                                    Modifier
                                        .padding((0.dp(), 0.dp(), 0.dp(), 16.dp()))
                                        .align(column, alignment),
                                    Alignment::TOP_START,
                                    false,
                                    compose_runtime::callback(move |_| title()),
                                );
                            },
                        );
                    }
                    if let Some(text) = parts.text.clone() {
                        provide_content_color_text_style(
                            parts.text_content_color,
                            typography.body_medium.clone(),
                            || {
                                // What's left once the title and buttons have their room.
                                Box(
                                    Modifier
                                        .weight_with(column, 1.0, false)
                                        .padding((0.dp(), 0.dp(), 0.dp(), 24.dp()))
                                        .align(column, compose_ui::Alignment::START),
                                    Alignment::TOP_START,
                                    false,
                                    compose_runtime::callback(move |_| text()),
                                );
                            },
                        );
                    }
                    let (confirm, dismiss) =
                        (parts.confirm_button.clone(), parts.dismiss_button.clone());
                    Box(
                        Modifier.align(column, compose_ui::Alignment::END),
                        Alignment::TOP_START,
                        false,
                        compose_runtime::callback(move |_| {
                            let (confirm, dismiss) = (confirm.clone(), dismiss.clone());
                            // Note: a text button takes its own colors, not this one.
                            provide_content_color_text_style(
                                MaterialTheme::color_scheme().primary,
                                MaterialTheme::typography().label_large.clone(),
                                || alert_dialog_buttons(confirm, dismiss),
                            );
                        }),
                    );
                }),
            );
        }),
    );
}

/// The buttons, as Compose's `AlertDialogFlowRow`: a flow row in the other layout direction,
/// so the confirm button comes last on a row, first when they stack; 8 dp apart along a row,
/// and across rows 8 dp less what the touch target adds to a button (`ButtonDefaults.MinHeight`
/// of 40 dp).
fn alert_dialog_buttons(
    confirm: compose_runtime::Callback,
    dismiss: Option<compose_runtime::Callback>,
) {
    let minimum = crate::LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE.current();
    let from_touch_target = if minimum.is_specified() { minimum.0 } else { 0.0 } - 40.0;
    let cross =
        Dp((BUTTONS_CROSS_AXIS_SPACING.0 - from_touch_target)
            .clamp(0.0, BUTTONS_CROSS_AXIS_SPACING.0));
    let policy = compose_runtime::remember_keyed(cross.0.to_bits(), move || {
        Rc::new(ButtonsFlowRow { main: BUTTONS_MAIN_AXIS_SPACING, cross }) as Rc<dyn MeasurePolicy>
    });
    Layout(
        Modifier,
        policy,
        compose_runtime::callback(move || {
            confirm();
            if let Some(dismiss) = &dismiss {
                dismiss();
            }
        }),
    );
}

/// `AlertDialogFlowRow`'s layout: the buttons in rows as wide as allowed, `main` apart, rows
/// `cross` apart, as a `FlowRow` with `spacedBy` arrangements in the other layout direction
/// (each row from the end).
struct ButtonsFlowRow {
    main: Dp,
    cross: Dp,
}

impl MeasurePolicy for ButtonsFlowRow {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let density = scope.density();
        let main = density.round_to_px(self.main);
        let cross = density.round_to_px(self.cross);
        let loose = Constraints::new(0, constraints.max_width, 0, constraints.max_height);
        let placeables: Vec<Placeable> =
            measurables.iter().map(|measurable| scope.measure(*measurable, loose)).collect();
        // The rows: the placeables' indexes, a new row when the next doesn't fit.
        let mut rows: Vec<(Vec<usize>, i32, i32)> = Vec::new();
        for (index, placeable) in placeables.iter().enumerate() {
            let fits = rows.last().is_some_and(|(_, width, _)| {
                width + main + placeable.width <= constraints.max_width
            });
            match rows.last_mut() {
                Some((items, width, height)) if fits => {
                    items.push(index);
                    *width += main + placeable.width;
                    *height = (*height).max(placeable.height);
                }
                _ => rows.push((vec![index], placeable.width, placeable.height)),
            }
        }
        let width = rows
            .iter()
            .map(|(_, width, _)| *width)
            .max()
            .unwrap_or(0)
            .clamp(constraints.min_width, constraints.max_width);
        let height = rows.iter().map(|(_, _, height)| *height).sum::<i32>()
            + cross * (rows.len() as i32 - 1).max(0);
        let height = height.clamp(constraints.min_height, constraints.max_height);
        // In the other layout direction: from the right in a left-to-right layout.
        let from_right = scope.layout_direction() == LayoutDirection::Ltr;
        let mut y = 0;
        for (items, _, row_height) in &rows {
            let mut x = if from_right { width } else { 0 };
            for &index in items {
                let placeable = &placeables[index];
                if from_right {
                    x -= placeable.width;
                    scope.place(placeable, x, y);
                    x -= main;
                } else {
                    scope.place(placeable, x, y);
                    x += placeable.width + main;
                }
            }
            y += row_height + cross;
        }
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "AlertDialogFlowRow"
    }
}

// ---- DropdownMenu ----------------------------------------------------------------------------

/// How far a menu stays from the window's top and bottom, as Compose's `MenuVerticalMargin`.
const MENU_VERTICAL_MARGIN: Dp = Dp(48.0);

/// Places a menu as Compose's `DropdownMenuPositionProvider`: below its anchor at its start
/// (moved by `offset`, in pixels: the menu's `DpOffset` rounded at the density), else ending
/// with the anchor's end, else at the window's end; below, else above, else centered on the
/// anchor's top, else at the window's bottom, keeping 48 dp from the window's top and bottom.
struct DropdownMenuPosition {
    offset: IntOffset,
    vertical_margin: i32,
    /// Where the menu grows from, set as it's placed, as the provider's `transformOrigin`.
    transform_origin: Cell<TransformOrigin>,
}

/// The point of the menu nearest its anchor, as fractions of its size, as Compose's
/// `calculateTransformOrigin`: the side facing the anchor, or the middle of their overlap.
fn transform_origin(anchor: Rect, menu: Rect) -> TransformOrigin {
    fn pivot(anchor: (f32, f32), menu: (f32, f32)) -> f32 {
        let size = menu.1 - menu.0;
        if menu.0 >= anchor.1 {
            0.0
        } else if menu.1 <= anchor.0 {
            1.0
        } else if size == 0.0 {
            0.0
        } else {
            // Integer pixels, as Compose's `IntRect`s.
            let center = ((anchor.0.max(menu.0) + anchor.1.min(menu.1)) / 2.0).trunc();
            (center - menu.0) / size
        }
    }
    let round = |rect: Rect| {
        Rect::new(rect.left.round(), rect.top.round(), rect.right.round(), rect.bottom.round())
    };
    let (anchor, menu) = (round(anchor), round(menu));
    TransformOrigin {
        pivot_fraction_x: pivot((anchor.left, anchor.right), (menu.left, menu.right)),
        pivot_fraction_y: pivot((anchor.top, anchor.bottom), (menu.top, menu.bottom)),
    }
}

impl PopupPositionProvider for DropdownMenuPosition {
    fn position(
        &self,
        anchor: Rect,
        window: IntSize,
        layout_direction: LayoutDirection,
        popup: IntSize,
    ) -> IntOffset {
        let (left, right) = (anchor.left.round() as i32, anchor.right.round() as i32);
        let (top, bottom) = (anchor.top.round() as i32, anchor.bottom.round() as i32);
        let ltr = layout_direction == LayoutDirection::Ltr;
        // The offset goes toward the end: left in a right-to-left layout.
        let offset_x = if ltr { self.offset.x } else { -self.offset.x };
        let left_to_anchor_left = left + offset_x;
        let right_to_anchor_right = right + offset_x - popup.width;
        let right_to_window_right = window.width - popup.width;
        let candidates = if ltr {
            [
                left_to_anchor_left,
                right_to_anchor_right,
                if left >= 0 { right_to_window_right } else { 0 },
            ]
        } else {
            [
                right_to_anchor_right,
                left_to_anchor_left,
                if right <= window.width { 0 } else { right_to_window_right },
            ]
        };
        let x = candidates
            .into_iter()
            .find(|&x| x >= 0 && x + popup.width <= window.width)
            .unwrap_or(candidates[1]);
        let margin = self.vertical_margin;
        let top_to_anchor_bottom = (bottom + self.offset.y).max(margin);
        let bottom_to_anchor_top = top - self.offset.y - popup.height;
        let center_to_anchor_top = top - popup.height / 2;
        let bottom_to_window_bottom = window.height - popup.height - margin;
        let y = [
            top_to_anchor_bottom,
            bottom_to_anchor_top,
            center_to_anchor_top,
            bottom_to_window_bottom,
        ]
        .into_iter()
        .find(|&y| y >= margin && y + popup.height <= window.height - margin)
        .unwrap_or(bottom_to_anchor_top);
        // Kept in the window, as a popup window clipped to the screen.
        let position = IntOffset::new(
            x.min(window.width - popup.width).max(0),
            y.min(window.height - popup.height).max(0),
        );
        let menu = Rect::new(
            position.x as f32,
            position.y as f32,
            (position.x + popup.width) as f32,
            (position.y + popup.height) as f32,
        );
        self.transform_origin.set(transform_origin(anchor, menu));
        position
    }
}

/// As Compose's `MenuDefaults`: the stable menu's values.
pub struct MenuDefaults;

impl MenuDefaults {
    /// A focusable popup, as `DefaultMenuProperties`.
    pub const DEFAULT_MENU_PROPERTIES: PopupProperties = PopupProperties {
        focusable: true,
        dismiss_on_back_press: true,
        dismiss_on_click_outside: true,
    };
    /// No tonal elevation, as `TonalElevation`.
    pub const TONAL_ELEVATION: Dp = Dp(0.0);
    /// 3 dp, as `ShadowElevation` (`MenuTokens.ContainerElevation`, level 2).
    pub const SHADOW_ELEVATION: Dp = Dp(3.0);
    /// 20 dp, as `LeadingIconSize`.
    pub const LEADING_ICON_SIZE: Dp = Dp(20.0);
    /// 20 dp, as `TrailingIconSize` (without precision pointer sizing).
    pub const TRAILING_ICON_SIZE: Dp = Dp(20.0);
    /// 12 dp at the start and end, as `DropdownMenuItemContentPadding`.
    pub const DROPDOWN_MENU_ITEM_CONTENT_PADDING: PaddingValues = PaddingValues::new(
        MENU_ITEM_HORIZONTAL_PADDING,
        Dp(0.0),
        MENU_ITEM_HORIZONTAL_PADDING,
        Dp(0.0),
    );

    /// The extra-small shape, as `shape`.
    pub fn shape() -> Shape {
        MaterialTheme::shapes().extra_small.clone()
    }

    /// The surface container, as `containerColor`.
    pub fn container_color() -> Color {
        MaterialTheme::color_scheme().surface_container
    }

    /// The items' colors, as `itemColors()`: the text in the on-surface color, the icons in its
    /// variant, all the on-surface color at 38 % when disabled (`ListTokens`).
    pub fn item_colors() -> MenuItemColors {
        let scheme = MaterialTheme::color_scheme();
        let disabled = scheme.on_surface.with_alpha(0.38);
        MenuItemColors {
            text_color: scheme.on_surface,
            leading_icon_color: scheme.on_surface_variant,
            trailing_icon_color: scheme.on_surface_variant,
            disabled_text_color: disabled,
            disabled_leading_icon_color: disabled,
            disabled_trailing_icon_color: disabled,
        }
    }

    /// [`item_colors`](Self::item_colors) with the colors given (`Color::UNSPECIFIED` keeps a
    /// default), as `itemColors(textColor, …)`.
    pub fn item_colors_with(
        text_color: Color,
        leading_icon_color: Color,
        trailing_icon_color: Color,
        disabled_text_color: Color,
        disabled_leading_icon_color: Color,
        disabled_trailing_icon_color: Color,
    ) -> MenuItemColors {
        let base = Self::item_colors();
        let or = |color: Color, default: Color| if color.is_specified() { color } else { default };
        MenuItemColors {
            text_color: or(text_color, base.text_color),
            leading_icon_color: or(leading_icon_color, base.leading_icon_color),
            trailing_icon_color: or(trailing_icon_color, base.trailing_icon_color),
            disabled_text_color: or(disabled_text_color, base.disabled_text_color),
            disabled_leading_icon_color: or(
                disabled_leading_icon_color,
                base.disabled_leading_icon_color,
            ),
            disabled_trailing_icon_color: or(
                disabled_trailing_icon_color,
                base.disabled_trailing_icon_color,
            ),
        }
    }
}

/// A menu item's text and icon colors, enabled and not, as Compose's `MenuItemColors` (its
/// container colors are the expressive items').
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MenuItemColors {
    pub text_color: Color,
    pub leading_icon_color: Color,
    pub trailing_icon_color: Color,
    pub disabled_text_color: Color,
    pub disabled_leading_icon_color: Color,
    pub disabled_trailing_icon_color: Color,
}

impl MenuItemColors {
    /// As `textColor(enabled)`.
    pub fn text_color(&self, enabled: bool) -> Color {
        if enabled { self.text_color } else { self.disabled_text_color }
    }

    /// As `leadingIconColor(enabled)`.
    pub fn leading_icon_color(&self, enabled: bool) -> Color {
        if enabled { self.leading_icon_color } else { self.disabled_leading_icon_color }
    }

    /// As `trailingIconColor(enabled)`.
    pub fn trailing_icon_color(&self, enabled: bool) -> Color {
        if enabled { self.trailing_icon_color } else { self.disabled_trailing_icon_color }
    }
}

impl compose_runtime::ComposeArg for MenuItemColors {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// An item's padding at its start and end, and between its text and icons, as Compose's
/// `DropdownMenuItemHorizontalPadding`.
const MENU_ITEM_HORIZONTAL_PADDING: Dp = Dp(12.0);
/// The width an item's icon takes at least, as `ListTokens.ItemLeadingIconSize` and
/// `ItemTrailingIconSize`.
const MENU_ITEM_ICON_SLOT: Dp = Dp(24.0);
/// A menu's size and opacity as it opens or closes, as Compose's `ClosedScaleTarget` and
/// `ClosedAlphaTarget`.
const CLOSED_SCALE: f32 = 0.8;
const CLOSED_ALPHA: f32 = 0.0;

/// A menu shown while `expanded`, below its anchor (the layout around it), as Compose's
/// `DropdownMenu`: on `shape` in `container_color` (by default the extra-small shape in the
/// surface container, 3 dp of shadow), as wide as its widest item, 8 dp above and below its
/// items, scrolling if taller than the window. It grows in from its anchor's side (from 0.8 of
/// its size, fading in) and shrinks out the same way before it leaves; it takes the focus (its
/// `properties`), and a click outside or Escape calls `on_dismiss_request`.
#[composable]
#[allow(non_snake_case)]
pub fn DropdownMenu(
    expanded: bool,
    on_dismiss_request: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::DpOffset::ZERO)] offset: DpOffset,
    #[default(crate::remember_scroll_state(0))] scroll_state: compose_foundation::ScrollState,
    #[default(crate::MenuDefaults::DEFAULT_MENU_PROPERTIES)] properties: PopupProperties,
    #[default(crate::MenuDefaults::shape())] shape: Shape,
    #[default(crate::MenuDefaults::container_color())] container_color: Color,
    #[default(crate::MenuDefaults::TONAL_ELEVATION)] tonal_elevation: Dp,
    #[default(crate::MenuDefaults::SHADOW_ELEVATION)] shadow_elevation: Dp,
    #[default(None)] border: Option<BorderStroke>,
    content: impl Fn(ColumnScope) + 'static,
) {
    // Shown from when it's expanded to when it has animated out, as Compose's
    // `MutableTransitionState(false)` the menu's transition follows.
    let expanded_state = remember(|| MutableTransitionState::new(false));
    expanded_state.set_target_state(expanded);
    if expanded_state.current_state() || expanded_state.target_state() {
        let density = compose_ui::LOCAL_DENSITY.current();
        let vertical_margin = density.round_to_px(MENU_VERTICAL_MARGIN);
        // In pixels at the density, as Compose's `contentOffset.x.roundToPx()`.
        let offset = IntOffset::new(density.round_to_px(offset.x), density.round_to_px(offset.y));
        let position = compose_runtime::remember_keyed(OffsetKey(offset, vertical_margin), || {
            Rc::new(DropdownMenuPosition {
                offset,
                vertical_margin,
                transform_origin: Cell::new(TransformOrigin::CENTER),
            })
        });
        let provider = position.clone() as Rc<dyn PopupPositionProvider>;
        let scroll = scroll_state;
        compose_ui::PopupWithPositionProvider(
            provider,
            Some(on_dismiss_request),
            properties,
            compose_runtime::callback(move || {
                let (modifier, content, scroll) = (modifier.clone(), content.clone(), scroll);
                // The open and close animation, as Compose's `DropdownMenuContent`.
                let transition = remember_transition(&expanded_state, "DropDownMenu");
                let scale = transition.animate_float(
                    |_| AnimationSpec::Spring(crate::motion::FAST_SPATIAL),
                    |expanded| if *expanded { 1.0 } else { CLOSED_SCALE },
                );
                let alpha = transition.animate_float(
                    |_| AnimationSpec::Spring(crate::motion::FAST_EFFECTS),
                    |expanded| if *expanded { 1.0 } else { CLOSED_ALPHA },
                );
                let position = position.clone();
                let layer = Modifier.graphics_layer(move |layer| {
                    let scale = scale.get();
                    layer.scale_x = scale;
                    layer.scale_y = scale;
                    layer.alpha = alpha.get();
                    layer.transform_origin = position.transform_origin.get();
                });
                Surface(
                    layer,
                    shape,
                    container_color,
                    Color::UNSPECIFIED,
                    tonal_elevation,
                    shadow_elevation,
                    border,
                    compose_runtime::callback(move || {
                        let content = content.clone();
                        // The modifier on the items' column, its 8 dp padding outside the
                        // scroll, as wide as its widest item, as Compose's
                        // `DropdownMenuContent`.
                        Column(
                            modifier
                                .clone()
                                .padding((0.dp(), 8.dp()))
                                .width(compose_ui::IntrinsicSize::Max)
                                .vertical_scroll(scroll),
                            Arrangement::TOP,
                            Alignment::START,
                            compose_runtime::callback(move |column| content(column)),
                        );
                    }),
                );
            }),
        );
    }
}

#[derive(Clone, Copy, PartialEq)]
struct OffsetKey(IntOffset, i32);

impl compose_runtime::ComposeArg for OffsetKey {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// An item of a menu, as Compose's `DropdownMenuItem`: filling the menu's width (112 to 280 dp
/// when the menu asks how wide it would be), at least 48 dp high, `content_padding` in (12 dp
/// at its ends), its text in the label-large style, with optional icons at its ends (each at
/// least 24 dp wide, 12 dp from the text) in `colors`.
#[composable]
#[allow(non_snake_case)]
pub fn DropdownMenuItem(
    text: impl Fn() + 'static,
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] leading_icon: Option<impl Fn() + 'static>,
    #[default(None)] trailing_icon: Option<impl Fn() + 'static>,
    #[default(true)] enabled: bool,
    #[default(crate::MenuDefaults::item_colors())] colors: MenuItemColors,
    #[default(crate::MenuDefaults::DROPDOWN_MENU_ITEM_CONTENT_PADDING)]
    content_padding: PaddingValues,
    #[default(None)] interaction_source: Option<MutableInteractionSource>,
) {
    let layout = {
        let leading = leading_icon.is_some();
        compose_runtime::remember_keyed(leading, move || {
            Rc::new(MenuItemLayout { leading }) as Rc<dyn MeasurePolicy>
        })
    };
    // The ripple in the content color around the item, as Compose's `ripple(true)`.
    let indication = crate::ripple::default_ripple();
    let modifier = modifier
        .clickable_with(
            ClickableOptions {
                enabled,
                indication,
                interaction_source,
                ..ClickableOptions::DEFAULT
            },
            on_click,
        )
        .fill_max_width()
        // The width it would like when the menu asks, as Compose's `sizeIn`.
        .size_in(compose_ui::SizeSpec::bounds(
            DROPDOWN_MENU_ITEM_MIN_WIDTH,
            48.dp(),
            DROPDOWN_MENU_ITEM_MAX_WIDTH,
            Dp::UNSPECIFIED,
        ))
        .padding(content_padding);
    let label = MaterialTheme::typography().label_large.clone();
    crate::ProvideTextStyle(
        label,
        compose_runtime::callback(move || {
            let (text, leading, trailing) =
                (text.clone(), leading_icon.clone(), trailing_icon.clone());
            Layout(
                modifier.clone(),
                layout.clone(),
                compose_runtime::callback(move || {
                    if let Some(leading) = leading.clone() {
                        let color = colors.leading_icon_color(enabled);
                        CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), &*leading);
                    }
                    let text = text.clone();
                    let color = colors.text_color(enabled);
                    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), || {
                        let text = text.clone();
                        Box(
                            Modifier,
                            Alignment::TOP_START,
                            false,
                            compose_runtime::callback(move |_| text()),
                        );
                    });
                    if let Some(trailing) = trailing.clone() {
                        let color = colors.trailing_icon_color(enabled);
                        CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), || {
                            trailing()
                        });
                    }
                }),
            );
        }),
    );
}

/// The width a menu item would like, as Compose's `DropdownMenuItemDefaultMinWidth` and
/// `DropdownMenuItemDefaultMaxWidth`.
const DROPDOWN_MENU_ITEM_MIN_WIDTH: Dp = Dp(112.0);
const DROPDOWN_MENU_ITEM_MAX_WIDTH: Dp = Dp(280.0);

/// A menu item's row, as Compose's: the leading icon, the text taking the rest of a bounded
/// width, and the trailing icon at the end, 12 dp apart and centered vertically, each icon at
/// the start of a slot at least 24 dp wide (`defaultMinSize`); at their natural widths when the
/// width is unbounded (the menu asking how wide it would be).
struct MenuItemLayout {
    /// The first child is the leading icon (the text the second).
    leading: bool,
}

impl MeasurePolicy for MenuItemLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let density = scope.density();
        let gap = density.round_to_px(MENU_ITEM_HORIZONTAL_PADDING);
        let icon_slot = density.round_to_px(MENU_ITEM_ICON_SLOT);
        let loose = Constraints::new(0, Constraints::INFINITY, 0, constraints.max_height);
        let count = measurables.len();
        let text_index = usize::from(self.leading).min(count.saturating_sub(1));
        let mut placeables: Vec<Option<Placeable>> = vec![None; count];
        let mut slots = vec![0; count];
        let mut fixed = 0;
        for (index, &child) in measurables.iter().enumerate() {
            if index != text_index {
                let placeable = scope.measure(child, loose);
                slots[index] = placeable.width.max(icon_slot);
                fixed += slots[index] + gap;
                placeables[index] = Some(placeable);
            }
        }
        if let Some(&text) = measurables.get(text_index) {
            let text_constraints = if constraints.max_width == Constraints::INFINITY {
                loose
            } else {
                let room = (constraints.max_width - fixed).max(0);
                Constraints::new(room, room, 0, constraints.max_height)
            };
            let placeable = scope.measure(text, text_constraints);
            slots[text_index] = placeable.width;
            placeables[text_index] = Some(placeable);
        }
        let content: i32 = slots.iter().sum::<i32>() + gap * (count.saturating_sub(1) as i32);
        let width = constraints.constrain_width(content);
        let height = constraints.constrain_height(
            placeables.iter().flatten().map(|placeable| placeable.height).max().unwrap_or(0),
        );
        let mut x = 0;
        for (placeable, slot) in placeables.iter().zip(&slots) {
            if let Some(placeable) = placeable {
                scope.place_relative(placeable, x, (height - placeable.height) / 2);
            }
            x += slot + gap;
        }
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "DropdownMenuItem"
    }
}
