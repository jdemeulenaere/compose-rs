//! `AlertDialog`, `BasicAlertDialog`, `DropdownMenu` and `DropdownMenuItem`, as Compose's
//! Material 3.

use compose_ui::ClickableOptions;
use std::rc::Rc;

use compose_foundation::ScrollModifiers as _;
use compose_foundation::layout::{Arrangement, Box, Column, ColumnScope};
use compose_runtime::{CompositionLocalProvider, composable, remember};
use compose_ui::{
    Alignment, Constraints, Dp, DpOffset, IntOffset, IntSize, Layout, LayoutDirection, Measurable,
    MeasurePolicy, MeasureResult, MeasureScope, Modifier, Placeable, PopupPositionProvider,
    PopupProperties, Rect, UnitExt,
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
        MaterialTheme::shapes().extra_large
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
        parts.shape,
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
                            typography.headline_small,
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
                            typography.body_medium,
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
                                MaterialTheme::typography().label_large,
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
        IntOffset::new(
            x.min(window.width - popup.width).max(0),
            y.min(window.height - popup.height).max(0),
        )
    }
}

/// A menu shown while `expanded`, below its anchor (the layout around it), as Compose's
/// `DropdownMenu`: on the extra-small shape in the surface container, as wide as its widest
/// item (112 to 280 dp), 8 dp above and below its items, scrolling if taller than the window;
/// it takes the focus, and a click outside or Escape calls `on_dismiss_request`.
#[composable]
#[allow(non_snake_case)]
pub fn DropdownMenu(
    expanded: bool,
    on_dismiss_request: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::DpOffset::ZERO)] offset: DpOffset,
    #[default(crate::remember_scroll_state(0))] scroll_state: compose_foundation::ScrollState,
    content: impl Fn(ColumnScope) + 'static,
) {
    if expanded {
        let density = compose_ui::LOCAL_DENSITY.current();
        let vertical_margin = density.round_to_px(MENU_VERTICAL_MARGIN);
        // In pixels at the density, as Compose's `contentOffset.x.roundToPx()`.
        let offset = IntOffset::new(density.round_to_px(offset.x), density.round_to_px(offset.y));
        let provider = compose_runtime::remember_keyed(OffsetKey(offset, vertical_margin), || {
            Rc::new(DropdownMenuPosition { offset, vertical_margin })
                as Rc<dyn PopupPositionProvider>
        });
        let column = remember(|| Rc::new(MenuColumn) as Rc<dyn MeasurePolicy>);
        let scroll = scroll_state;
        let scheme = MaterialTheme::color_scheme();
        compose_ui::PopupWithPositionProvider(
            provider,
            Some(on_dismiss_request),
            PopupProperties { focusable: true, ..PopupProperties::default() },
            compose_runtime::callback(move || {
                let (modifier, content, column, scroll) =
                    (modifier.clone(), content.clone(), column.clone(), scroll);
                Surface(
                    Modifier,
                    MaterialTheme::shapes().extra_small,
                    scheme.surface_container,
                    compose_ui::Color::UNSPECIFIED,
                    Dp(0.0),
                    // As `MenuDefaults.ShadowElevation`.
                    3.dp(),
                    None,
                    compose_runtime::callback(move || {
                        let content = content.clone();
                        // The modifier on the items' column, its 8 dp padding outside the
                        // scroll, as Compose's `DropdownMenuContent`.
                        Layout(
                            modifier.clone().padding((0.dp(), 8.dp())).vertical_scroll(scroll),
                            column.clone(),
                            compose_runtime::callback(move || content(ColumnScope::__new())),
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

/// A menu's items, one under the other, all as wide as the widest (at its natural width), at
/// least 112 dp and at most 280 dp, as Compose's `width(IntrinsicSize.Max)` menu.
struct MenuColumn;

impl MeasurePolicy for MenuColumn {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let density = scope.density();
        let min = density.round_to_px(112.dp()).min(constraints.max_width);
        let max = density.round_to_px(280.dp()).min(constraints.max_width).max(min);
        // Each item's natural width first, then all at the widest.
        let widest = measurables
            .iter()
            .map(|&child| scope.max_intrinsic_width(child, Constraints::INFINITY))
            .max()
            .unwrap_or(0);
        // A width given to the menu wins, as `width(IntrinsicSize.Max)` within it.
        let width = widest.clamp(min, max).clamp(constraints.min_width, constraints.max_width);
        let item = Constraints::new(width, width, 0, Constraints::INFINITY);
        let placeables: Vec<Placeable> =
            measurables.iter().map(|&child| scope.measure(child, item)).collect();
        let mut y = 0;
        for placeable in &placeables {
            scope.place_relative(placeable, 0, y);
            y += placeable.height;
        }
        scope.layout(constraints.constrain_width(width), constraints.constrain_height(y))
    }

    fn debug_name(&self) -> &'static str {
        "DropdownMenu"
    }
}

/// An item of a menu, as Compose's `DropdownMenuItem`: at least 48 dp high, 12 dp in, its text
/// in the label-large style, with optional icons at its ends.
#[composable]
#[allow(non_snake_case)]
pub fn DropdownMenuItem(
    text: impl Fn() + 'static,
    on_click: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] leading_icon: Option<impl Fn() + 'static>,
    #[default(None)] trailing_icon: Option<impl Fn() + 'static>,
    #[default(true)] enabled: bool,
) {
    let scheme = MaterialTheme::color_scheme();
    let color = if enabled { scheme.on_surface } else { scheme.on_surface.with_alpha(0.38) };
    let label = MaterialTheme::typography().label_large.clone();
    let layout = {
        let leading = leading_icon.is_some();
        compose_runtime::remember_keyed(leading, move || {
            Rc::new(MenuItemLayout { leading }) as Rc<dyn MeasurePolicy>
        })
    };
    let modifier = modifier
        .clickable_with(
            ClickableOptions { enabled, indication: color.into(), ..ClickableOptions::DEFAULT },
            on_click,
        )
        .fill_max_width()
        .size_in(compose_ui::SizeSpec::bounds(
            Dp::UNSPECIFIED,
            48.dp(),
            Dp::UNSPECIFIED,
            Dp::UNSPECIFIED,
        ))
        .padding((12.dp(), 0.dp()));
    CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(color), || {
        CompositionLocalProvider(LOCAL_TEXT_STYLE.provides(label), || {
            let (text, leading, trailing) =
                (text.clone(), leading_icon.clone(), trailing_icon.clone());
            Layout(
                modifier.clone(),
                layout.clone(),
                compose_runtime::callback(move || {
                    if let Some(leading) = leading.clone() {
                        leading();
                    }
                    let text = text.clone();
                    Box(
                        Modifier,
                        Alignment::CENTER_START,
                        false,
                        compose_runtime::callback(move |_| text()),
                    );
                    if let Some(trailing) = trailing.clone() {
                        trailing();
                    }
                }),
            );
        });
    });
}

/// A menu item's row: the leading icon, the text taking the rest of a bounded width, and the
/// trailing icon at the end, 12 dp apart and centered vertically; at their natural widths when
/// the width is unbounded (the menu asking how wide it would be).
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
        let gap = scope.density().round_to_px(12.dp());
        let loose = Constraints::new(0, Constraints::INFINITY, 0, constraints.max_height);
        let count = measurables.len();
        let text_index = usize::from(self.leading).min(count.saturating_sub(1));
        let mut placeables: Vec<Option<Placeable>> = vec![None; count];
        let mut fixed = 0;
        for (index, &child) in measurables.iter().enumerate() {
            if index != text_index {
                let placeable = scope.measure(child, loose);
                fixed += placeable.width + gap;
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
            placeables[text_index] = Some(scope.measure(text, text_constraints));
        }
        let content: i32 =
            placeables.iter().flatten().map(|placeable| placeable.width).sum::<i32>()
                + gap * (count.saturating_sub(1) as i32);
        let width = constraints.constrain_width(content);
        let height = constraints.constrain_height(
            placeables.iter().flatten().map(|placeable| placeable.height).max().unwrap_or(0),
        );
        let mut x = 0;
        for placeable in placeables.iter().flatten() {
            scope.place_relative(placeable, x, (height - placeable.height) / 2);
            x += placeable.width + gap;
        }
        scope.layout(width, height)
    }

    fn debug_name(&self) -> &'static str {
        "DropdownMenuItem"
    }
}
