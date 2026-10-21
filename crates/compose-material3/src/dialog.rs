//! `AlertDialog`, `DropdownMenu` and `DropdownMenuItem`, as Compose's Material 3.

use std::rc::Rc;

use compose_foundation::layout::{Arrangement, Box, Column, ColumnScope, Row};
use compose_runtime::{CompositionLocalProvider, composable, remember};
use compose_ui::{
    Alignment, Constraints, Dp, IntOffset, IntSize, Layout, Measurable, MeasurePolicy,
    MeasureResult, MeasureScope, Modifier, Placeable, PopupPositionProvider, PopupProperties, Rect,
    Role, UnitExt,
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
                                                Box(
                                                    Modifier.padding_sides(
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

/// Places a menu below its anchor, at its start (moved by `offset`), above it if there's no room
/// below, and within the window, as Compose's `DropdownMenuPositionProvider`.
struct DropdownMenuPosition {
    offset: IntOffset,
}

impl PopupPositionProvider for DropdownMenuPosition {
    fn position(&self, anchor: Rect, window: IntSize, popup: IntSize) -> IntOffset {
        let below = anchor.bottom.round() as i32 + self.offset.y;
        let above = anchor.top.round() as i32 - self.offset.y - popup.height;
        let y = if below + popup.height <= window.height || above < 0 { below } else { above };
        let x = (anchor.left.round() as i32 + self.offset.x).min(window.width - popup.width).max(0);
        IntOffset::new(x, y.min(window.height - popup.height).max(0))
    }
}

/// A menu shown while `expanded`, below its anchor (the layout around it), as Compose's
/// `DropdownMenu`: on the extra-small shape in the surface container, 112 to 280 dp wide, 8 dp
/// above and below its items; it takes the focus, and a click outside or Escape calls
/// `on_dismiss_request`.
#[composable]
#[allow(non_snake_case)]
pub fn DropdownMenu(
    expanded: bool,
    on_dismiss_request: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::IntOffset::ZERO)] offset: IntOffset,
    content: impl Fn(ColumnScope) + 'static,
) {
    if expanded {
        let provider = compose_runtime::remember_keyed(OffsetKey(offset), || {
            Rc::new(DropdownMenuPosition { offset }) as Rc<dyn PopupPositionProvider>
        });
        let column = remember(|| Rc::new(MenuColumn) as Rc<dyn MeasurePolicy>);
        let scheme = MaterialTheme::color_scheme();
        compose_ui::PopupWithPositionProvider(
            provider,
            Some(on_dismiss_request),
            PopupProperties { focusable: true, ..PopupProperties::default() },
            compose_runtime::callback(move || {
                let (modifier, content, column) =
                    (modifier.clone(), content.clone(), column.clone());
                Surface(
                    modifier,
                    MaterialTheme::shapes().extra_small,
                    scheme.surface_container,
                    compose_ui::Color::UNSPECIFIED,
                    None,
                    compose_runtime::callback(move || {
                        let content = content.clone();
                        Layout(
                            Modifier.padding_horizontal_vertical(0.dp(), 8.dp()),
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
struct OffsetKey(IntOffset);

impl compose_runtime::ComposeArg for OffsetKey {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A menu's items, one under the other, each at least the menu's minimum width (112 dp), at
/// most 280 dp.
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
        let item = Constraints::new(min, max, 0, Constraints::INFINITY);
        let placeables: Vec<Placeable> =
            measurables.iter().map(|&child| scope.measure(child, item)).collect();
        let width = placeables.iter().map(|placeable| placeable.width).max().unwrap_or(min);
        let mut y = 0;
        for placeable in &placeables {
            scope.place(placeable, 0, y);
            y += placeable.height;
        }
        scope.layout(constraints.constrain_width(width), constraints.constrain_height(y))
    }

    fn debug_name(&self) -> &'static str {
        "DropdownMenu"
    }
}

/// An item of a menu, as Compose's `DropdownMenuItem`: 48 dp high, 12 dp in, its text in the
/// label-large style, with optional icons at its ends.
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
            Row(
                modifier.clone(),
                Arrangement::spaced_by(12.dp()),
                Alignment::CENTER_VERTICALLY,
                compose_runtime::callback(move |row| {
                    if let Some(leading) = leading.clone() {
                        leading();
                    }
                    let text = text.clone();
                    Box(
                        Modifier.weight(row, 1.0),
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
