//! The dropdown context menu's look, as Compose's internal `ContextMenuUi.kt`: Material 3's menu
//! sizes (`ContextMenuSpec`), its colors, its scope of items and separators, the column, an item
//! and the popup (D2036).

use std::rc::Rc;

use compose_runtime::composable;
use compose_ui::{
    Alignment, ClickableOptions, Color, Dp, FontWeight, Modifier, PaddingValues,
    PopupPositionProvider, PopupProperties, SizeSpec, TextAlign, TextStyle, TextUnit,
    VerticalAlignment,
};

use crate::layout::Arrangement;
use crate::{ScrollModifiers, remember_scroll_state};

/// Material 3's menu sizes and text, as Compose's `ContextMenuSpec` (its `m3.material.io` menu
/// spec).
pub struct ContextMenuSpec;

impl ContextMenuSpec {
    pub const CONTAINER_WIDTH_MIN: Dp = Dp(112.0);
    pub const CONTAINER_WIDTH_MAX: Dp = Dp(280.0);
    pub const LIST_ITEM_HEIGHT: Dp = Dp(48.0);
    pub const MENU_CONTAINER_ELEVATION: Dp = Dp(3.0);
    pub const CORNER_RADIUS: Dp = Dp(4.0);
    pub const LABEL_VERTICAL_TEXT_ALIGNMENT: VerticalAlignment = Alignment::CENTER_VERTICALLY;
    pub const LABEL_HORIZONTAL_TEXT_ALIGNMENT: TextAlign = TextAlign::Start;
    /// Left and right of the column, and between an item's icon and label.
    pub const HORIZONTAL_PADDING: Dp = Dp(12.0);
    /// Above and below the column.
    pub const VERTICAL_PADDING: Dp = Dp(8.0);
    pub const ICON_SIZE: Dp = Dp(24.0);
    pub const DIVIDER_HEIGHT: Dp = Dp(1.0);
    pub const DIVIDER_VERTICAL_PADDING: Dp = Dp(8.0);
    pub const FONT_SIZE: TextUnit = TextUnit::sp(14.0);
    pub const FONT_WEIGHT: FontWeight = FontWeight::MEDIUM;
    pub const LINE_HEIGHT: TextUnit = TextUnit::sp(20.0);
    pub const LETTER_SPACING: TextUnit = TextUnit::sp(0.1);

    /// An item's label's style in `color`, as Compose's `textStyle`.
    pub fn text_style(color: Color) -> TextStyle {
        TextStyle {
            color,
            text_align: Some(Self::LABEL_HORIZONTAL_TEXT_ALIGNMENT),
            font_size: Self::FONT_SIZE,
            font_weight: Some(Self::FONT_WEIGHT),
            line_height: Self::LINE_HEIGHT,
            letter_spacing: Self::LETTER_SPACING,
            ..TextStyle::DEFAULT
        }
    }
}

/// A context menu's colors, as Compose's `ContextMenuColors`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ContextMenuColors {
    pub background_color: Color,
    pub text_color: Color,
    pub icon_color: Color,
    pub disabled_text_color: Color,
    pub disabled_icon_color: Color,
}

/// Black at Compose's alpha of what's disabled (0.38: 97 of 255, as `Color.copy(alpha = 0.38f)`
/// packs it).
const DISABLED_BLACK: Color = Color::from_argb(0x6100_0000);

/// The colors without a platform's, as Compose's `DefaultContextMenuColors`.
pub const DEFAULT_CONTEXT_MENU_COLORS: ContextMenuColors = ContextMenuColors {
    background_color: Color::WHITE,
    text_color: Color::BLACK,
    icon_color: Color::BLACK,
    disabled_text_color: DISABLED_BLACK,
    disabled_icon_color: DISABLED_BLACK,
};

/// The platform's menu colors, as Compose's `computeContextMenuColors`: Android's resolve the
/// theme's `Widget.PopupMenu` styles, which compose-rs doesn't read, so these are the defaults on
/// every platform (D2036).
pub fn compute_context_menu_colors() -> ContextMenuColors {
    DEFAULT_CONTEXT_MENU_COLORS
}

impl compose_runtime::ComposeArg for ContextMenuColors {
    fn same(&self, other: &ContextMenuColors) -> bool {
        self == other
    }
}

/// An item's leading icon, told the color to draw with (the enabled or the disabled one).
pub type ContextMenuIcon = Rc<dyn Fn(Color)>;

/// What adds a menu's items, as Compose's `contextMenuBuilderBlock: ContextMenuScope.() -> Unit`.
pub type ContextMenuBuilderBlock = Rc<dyn Fn(&mut ContextMenuScope)>;

struct Item {
    label: String,
    modifier: Modifier,
    enabled: bool,
    leading_icon: Option<ContextMenuIcon>,
    on_click: Rc<dyn Fn()>,
}

enum Component {
    Item(Box<Item>),
    Separator,
}

/// What a context menu's builder adds its items and separators to, as Compose's
/// `ContextMenuScope`: they're composed after the builder, in order.
#[derive(Default)]
pub struct ContextMenuScope {
    components: Vec<Component>,
}

impl ContextMenuScope {
    /// Adds an item: `label` (not blank), `enabled` (else not clickable, in the disabled colors),
    /// an icon before the label, measured at most `ICON_SIZE`, and what a click does (closing the
    /// menu is its own business), as Compose's `item`.
    pub fn item(
        &mut self,
        label: impl Into<String>,
        modifier: Modifier,
        enabled: bool,
        leading_icon: Option<ContextMenuIcon>,
        on_click: impl Fn() + 'static,
    ) {
        self.components.push(Component::Item(Box::new(Item {
            label: label.into(),
            modifier,
            enabled,
            leading_icon,
            on_click: Rc::new(on_click),
        })));
    }

    /// Adds a separator, a line across the menu, as Compose's `separator`.
    pub fn separator(&mut self) {
        self.components.push(Component::Separator);
    }

    /// Composes what was added, as Compose's `Content`.
    fn content(self, colors: ContextMenuColors) {
        for component in self.components {
            match component {
                Component::Item(item) => {
                    let Item { label, modifier, enabled, leading_icon, on_click } = *item;
                    assert!(!label.trim().is_empty(), "Label must not be blank");
                    ContextMenuItem(label, enabled, colors, modifier, leading_icon, on_click);
                }
                Component::Separator => crate::layout::Box(
                    Modifier
                        .padding(PaddingValues::symmetric(
                            Dp::ZERO,
                            ContextMenuSpec::DIVIDER_VERTICAL_PADDING,
                        ))
                        .fill_max_width()
                        .height(ContextMenuSpec::DIVIDER_HEIGHT)
                        .background(colors.icon_color),
                    Alignment::TOP_START,
                    false,
                    compose_runtime::callback(|_| {}),
                ),
            }
        }
    }
}

/// A popup of the items `block` adds, placed by `position_provider`, focusable, `on_dismiss`
/// called by a click outside or the back key, as Compose's `ContextMenuPopup`.
#[composable]
#[allow(non_snake_case)]
pub fn ContextMenuPopup(
    position_provider: Rc<dyn PopupPositionProvider>,
    on_dismiss: impl Fn() + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::context_menu::compute_context_menu_colors())] colors: ContextMenuColors,
    block: ContextMenuBuilderBlock,
) {
    let properties = PopupProperties { focusable: true, ..PopupProperties::default() };
    let block = block.clone();
    compose_ui::PopupWithPositionProvider(
        position_provider,
        Some(on_dismiss),
        properties,
        compose_runtime::callback(move || {
            ContextMenuColumnBuilder(modifier.clone(), colors, block.clone());
        }),
    );
}

/// The column of the items `block` adds, as Compose's `ContextMenuColumnBuilder`.
#[composable]
#[allow(non_snake_case)]
pub fn ContextMenuColumnBuilder(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::context_menu::DEFAULT_CONTEXT_MENU_COLORS)] colors: ContextMenuColors,
    block: ContextMenuBuilderBlock,
) {
    let block = block.clone();
    ContextMenuColumn(
        colors,
        modifier,
        compose_runtime::callback(move || {
            let mut scope = ContextMenuScope::default();
            block(&mut scope);
            scope.content(colors);
        }),
    );
}

/// The menu's column, as Compose's `ContextMenuColumn`: a shadow of its elevation around rounded
/// corners, its background, as wide as its widest item, padded above and below, scrolling.
#[composable]
#[allow(non_snake_case)]
pub fn ContextMenuColumn(
    colors: ContextMenuColors,
    #[default(crate::Modifier)] modifier: Modifier,
    content: impl Fn() + 'static,
) {
    let scroll = remember_scroll_state(0);
    let content = content.clone();
    crate::layout::Column(
        modifier
            .shadow_with(
                ContextMenuSpec::MENU_CONTAINER_ELEVATION,
                crate::shape::RoundedCornerShape(ContextMenuSpec::CORNER_RADIUS),
                true,
                compose_ui::DEFAULT_SHADOW_COLOR,
                compose_ui::DEFAULT_SHADOW_COLOR,
            )
            .background(colors.background_color)
            .width(compose_ui::IntrinsicSize::Max)
            .padding(PaddingValues::symmetric(Dp::ZERO, ContextMenuSpec::VERTICAL_PADDING))
            .vertical_scroll(scroll),
        Arrangement::TOP,
        Alignment::START,
        compose_runtime::callback(move |_| content()),
    );
}

/// An item of the menu, as Compose's `ContextMenuItem` (much as Material 3's
/// `DropdownMenuItemContent`): a row as wide as the menu, between its minimum and maximum widths,
/// `LIST_ITEM_HEIGHT` high, clickable when `enabled` (its click's label the label), its icon then
/// its label on one line, in the enabled or disabled colors.
#[composable]
#[allow(non_snake_case)]
pub fn ContextMenuItem(
    label: String,
    enabled: bool,
    colors: ContextMenuColors,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] leading_icon: Option<ContextMenuIcon>,
    on_click: Rc<dyn Fn()>,
) {
    let click = on_click.clone();
    let options = ClickableOptions {
        enabled,
        on_click_label: Some(label.clone().into()),
        ..ClickableOptions::DEFAULT
    };
    let row = modifier
        // Semantics may click a disabled item (tests do): checked again.
        .clickable_with(options, move || {
            if enabled {
                click();
            }
        })
        .fill_max_width()
        .size_in(SizeSpec {
            min_width: ContextMenuSpec::CONTAINER_WIDTH_MIN,
            min_height: ContextMenuSpec::LIST_ITEM_HEIGHT,
            max_width: ContextMenuSpec::CONTAINER_WIDTH_MAX,
            max_height: ContextMenuSpec::LIST_ITEM_HEIGHT,
            enforce_incoming: true,
        })
        .padding(PaddingValues::symmetric(ContextMenuSpec::HORIZONTAL_PADDING, Dp::ZERO));
    let (label, icon) = (label.clone(), leading_icon.clone());
    crate::layout::Row(
        row,
        Arrangement::spaced_by(ContextMenuSpec::HORIZONTAL_PADDING),
        ContextMenuSpec::LABEL_VERTICAL_TEXT_ALIGNMENT,
        compose_runtime::callback(move |scope| {
            if let Some(icon) = icon.clone() {
                let color = if enabled { colors.icon_color } else { colors.disabled_icon_color };
                crate::layout::Box(
                    Modifier.size_in(SizeSpec {
                        min_width: ContextMenuSpec::ICON_SIZE,
                        min_height: Dp::ZERO,
                        max_width: ContextMenuSpec::ICON_SIZE,
                        max_height: ContextMenuSpec::ICON_SIZE,
                        enforce_incoming: false,
                    }),
                    Alignment::TOP_START,
                    false,
                    compose_runtime::callback(move |_| icon(color)),
                );
            }
            let color = if enabled { colors.text_color } else { colors.disabled_text_color };
            crate::BasicText(
                label.clone().into(),
                Modifier.weight(scope, 1.0),
                ContextMenuSpec::text_style(color),
                None,
                compose_ui::TextOverflow::Clip,
                true,
                1,
                1,
            );
        }),
    );
}
