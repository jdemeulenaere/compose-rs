//! `AlertDialog`, `DropdownMenu` and `DropdownMenuItem`, as Compose's Material 3.

use std::rc::Rc;

use compose_foundation::ScrollModifiers as _;
use compose_foundation::layout::{Arrangement, Box, Column, ColumnScope, Row};
use compose_runtime::{CompositionLocalProvider, composable, remember};
use compose_ui::{
    Alignment, Constraints, Dp, IntOffset, IntSize, Layout, LayoutDirection, Measurable,
    MeasurePolicy, MeasureResult, MeasureScope, Modifier, Placeable, PopupPositionProvider,
    PopupProperties, Rect, Role, UnitExt,
};

use crate::components::Surface;
use crate::theme::{LOCAL_CONTENT_COLOR, LOCAL_TEXT_STYLE, MaterialTheme};

// ---- AlertDialog -----------------------------------------------------------------------------

/// A dialog with a title, text and buttons, as Compose's `AlertDialog`: on the extra-large
/// shape, in the high surface container, 24 dp in, 280 to 560 dp wide; the icon and title
/// centered above the text, the buttons at the end, 8 dp apart.
#[composable]
#[allow(non_snake_case)]
pub fn AlertDialog(
    on_dismiss_request: impl Fn() + 'static,
    confirm_button: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] dismiss_button: Option<impl Fn() + 'static>,
    #[default(None)] icon: Option<impl Fn() + 'static>,
    #[default(None)] title: Option<impl Fn() + 'static>,
    #[default(None)] text: Option<impl Fn() + 'static>,
    #[default(crate::window_default_dialog_properties())] properties: compose_ui::DialogProperties,
) {
    compose_ui::Dialog(
        on_dismiss_request,
        properties,
        compose_runtime::callback(move || {
            let scheme = MaterialTheme::color_scheme();
            let typography = MaterialTheme::typography();
            let (confirm, dismiss, icon, title, text) = (
                confirm_button.clone(),
                dismiss_button.clone(),
                icon.clone(),
                title.clone(),
                text.clone(),
            );
            Surface(
                modifier.clone().size_in(compose_ui::SizeSpec::bounds(
                    280.dp(),
                    Dp::UNSPECIFIED,
                    560.dp(),
                    Dp::UNSPECIFIED,
                )),
                MaterialTheme::shapes().extra_large,
                scheme.surface_container_high,
                compose_ui::Color::UNSPECIFIED,
                Dp(0.0),
                Dp(0.0),
                None,
                compose_runtime::callback(move || {
                    let (confirm, dismiss, icon, title, text) = (
                        confirm.clone(),
                        dismiss.clone(),
                        icon.clone(),
                        title.clone(),
                        text.clone(),
                    );
                    Column(
                        Modifier.padding(24.dp()),
                        Arrangement::TOP,
                        compose_ui::Alignment::START,
                        compose_runtime::callback(move |column: ColumnScope| {
                            if let Some(icon) = icon.clone() {
                                CompositionLocalProvider(
                                    LOCAL_CONTENT_COLOR.provides(scheme.secondary),
                                    || {
                                        Box(
                                            Modifier
                                                .align(
                                                    column,
                                                    compose_ui::Alignment::CENTER_HORIZONTALLY,
                                                )
                                                .padding_sides(0.dp(), 0.dp(), 0.dp(), 16.dp()),
                                            Alignment::CENTER,
                                            false,
                                            compose_runtime::callback({
                                                let icon = icon.clone();
                                                move |_| icon()
                                            }),
                                        );
                                    },
                                );
                            }
                            if let Some(title) = title.clone() {
                                let centered = icon.is_some();
                                CompositionLocalProvider(
                                    LOCAL_CONTENT_COLOR.provides(scheme.on_surface),
                                    || {
                                        CompositionLocalProvider(
                                            LOCAL_TEXT_STYLE.provides(typography.headline_small),
                                            || {
                                                let alignment = if centered {
                                                    compose_ui::Alignment::CENTER_HORIZONTALLY
                                                } else {
                                                    compose_ui::Alignment::START
                                                };
                                                Box(
                                                    Modifier
                                                        .align(column, alignment)
                                                        .padding_sides(
                                                            0.dp(),
                                                            0.dp(),
                                                            0.dp(),
                                                            16.dp(),
                                                        ),
                                                    Alignment::TOP_START,
                                                    false,
                                                    compose_runtime::callback({
                                                        let title = title.clone();
                                                        move |_| title()
                                                    }),
                                                );
                                            },
                                        );
                                    },
                                );
                            }
                            if let Some(text) = text.clone() {
                                CompositionLocalProvider(
                                    LOCAL_CONTENT_COLOR.provides(scheme.on_surface_variant),
                                    || {
                                        CompositionLocalProvider(
                                            LOCAL_TEXT_STYLE.provides(typography.body_medium),
                                            || {
                                                // What's left once the title and
                                                // buttons have their room.
                                                Box(
                                                    Modifier
                                                        .weight_filled(column, 1.0, false)
                                                        .padding_sides(
                                                            0.dp(),
                                                            0.dp(),
                                                            0.dp(),
                                                            24.dp(),
                                                        ),
                                                    Alignment::TOP_START,
                                                    false,
                                                    compose_runtime::callback({
                                                        let text = text.clone();
                                                        move |_| text()
                                                    }),
                                                );
                                            },
                                        );
                                    },
                                );
                            }
                            let (confirm, dismiss) = (confirm.clone(), dismiss.clone());
                            CompositionLocalProvider(
                                LOCAL_CONTENT_COLOR.provides(scheme.primary),
                                || {
                                    Row(
                                        Modifier.align(column, compose_ui::Alignment::END),
                                        Arrangement::spaced_by(8.dp()),
                                        Alignment::CENTER_VERTICALLY,
                                        compose_runtime::callback({
                                            let (confirm, dismiss) =
                                                (confirm.clone(), dismiss.clone());
                                            move |_| {
                                                if let Some(dismiss) = dismiss.clone() {
                                                    dismiss();
                                                }
                                                confirm();
                                            }
                                        }),
                                    );
                                },
                            );
                        }),
                    );
                }),
            );
        }),
    );
}

// ---- DropdownMenu ----------------------------------------------------------------------------

/// How far a menu stays from the window's top and bottom, as Compose's `MenuVerticalMargin`.
const MENU_VERTICAL_MARGIN: Dp = Dp(48.0);

/// Places a menu as Compose's `DropdownMenuPositionProvider`: below its anchor at its start
/// (moved by `offset`), else ending with the anchor's end, else at the window's end; below,
/// else above, else centered on the anchor's top, else at the window's bottom, keeping 48 dp
/// from the window's top and bottom.
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
    #[default(crate::IntOffset::ZERO)] offset: IntOffset,
    #[default(crate::remember_scroll_state())] scroll_state: compose_foundation::ScrollState,
    content: impl Fn(ColumnScope) + 'static,
) {
    if expanded {
        let density = compose_ui::LOCAL_DENSITY.current();
        let vertical_margin = density.round_to_px(MENU_VERTICAL_MARGIN);
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
                    (modifier.clone(), content.clone(), column.clone(), scroll.clone());
                Surface(
                    modifier,
                    MaterialTheme::shapes().extra_small,
                    scheme.surface_container,
                    compose_ui::Color::UNSPECIFIED,
                    Dp(0.0),
                    // As `MenuDefaults.ShadowElevation`.
                    3.dp(),
                    None,
                    compose_runtime::callback(move || {
                        let content = content.clone();
                        Layout(
                            Modifier
                                .vertical_scroll(scroll.clone())
                                .padding_horizontal_vertical(0.dp(), 8.dp()),
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
        let width = widest.clamp(min, max);
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
    let label = MaterialTheme::typography().label_large;
    let layout = {
        let leading = leading_icon.is_some();
        compose_runtime::remember_keyed(leading, move || {
            Rc::new(MenuItemLayout { leading }) as Rc<dyn MeasurePolicy>
        })
    };
    let modifier = modifier
        .clickable_indicated(enabled, Some(Role::Button), color, on_click)
        .fill_max_width()
        .size_in(compose_ui::SizeSpec::bounds(
            Dp::UNSPECIFIED,
            48.dp(),
            Dp::UNSPECIFIED,
            Dp::UNSPECIFIED,
        ))
        .padding_horizontal_vertical(12.dp(), 0.dp());
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
