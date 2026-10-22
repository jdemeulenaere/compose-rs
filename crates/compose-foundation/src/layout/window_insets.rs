//! Window insets, as Compose's `androidx.compose.foundation.layout.WindowInsets`: the space the
//! system draws over at the window's edges (status and navigation bars, the soft keyboard, a
//! display cutout), which the host reports (`UiHost::set_window_insets`; zero on desktop), and
//! the paddings that keep content clear of it (`Modifier.system_bars_padding()`, …).
//!
//! A [`WindowInsets`] names which insets (and which sides of them); its values are read where
//! it's used: the paddings read the host's insets while measuring (a change measures them
//! again, nothing recomposes), [`WindowInsets::as_padding_values`] in composition.

use compose_ui::{
    Constraints, Dp, Insets, LOCAL_DENSITY, LOCAL_LAYOUT_DIRECTION, LOCAL_PLATFORM_WINDOW_INSETS,
    LayoutDirection, LayoutModifierNode, Measurable, MeasureResult, MeasureScope, Modifier,
    ModifierNode, ModifierNodeElement, PaddingValues, PlatformWindowInsets,
};

/// Sides of the window, as Compose's `WindowInsetsSides`; combine them with `|`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct WindowInsetsSides(u8);

impl WindowInsetsSides {
    pub const NONE: WindowInsetsSides = WindowInsetsSides(0);
    pub const LEFT: WindowInsetsSides = WindowInsetsSides(1);
    pub const TOP: WindowInsetsSides = WindowInsetsSides(2);
    pub const RIGHT: WindowInsetsSides = WindowInsetsSides(4);
    pub const BOTTOM: WindowInsetsSides = WindowInsetsSides(8);
    pub const HORIZONTAL: WindowInsetsSides = WindowInsetsSides(1 | 4);
    pub const VERTICAL: WindowInsetsSides = WindowInsetsSides(2 | 8);
    pub const ALL: WindowInsetsSides = WindowInsetsSides(15);

    pub const fn contains(self, other: WindowInsetsSides) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for WindowInsetsSides {
    type Output = WindowInsetsSides;
    fn bitor(self, other: WindowInsetsSides) -> WindowInsetsSides {
        WindowInsetsSides(self.0 | other.0)
    }
}

/// The kinds of insets, in [`WindowInsets::sides`]' order.
const STATUS_BARS: usize = 0;
const NAVIGATION_BARS: usize = 1;
const CAPTION_BAR: usize = 2;
const IME: usize = 3;
const DISPLAY_CUTOUT: usize = 4;
const KINDS: usize = 5;

/// Which of the window's insets, as Compose's `WindowInsets.systemBars`, `ime`, …: for each kind,
/// the sides it covers. Combined with [`union`](Self::union) (the larger at each side) and
/// [`only`](Self::only).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct WindowInsets {
    sides: [WindowInsetsSides; KINDS],
}

impl compose_runtime::ComposeArg for WindowInsets {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl WindowInsets {
    const fn of(kind: usize) -> WindowInsets {
        let mut sides = [WindowInsetsSides::NONE; KINDS];
        sides[kind] = WindowInsetsSides::ALL;
        WindowInsets { sides }
    }

    /// No insets.
    pub const NONE: WindowInsets = WindowInsets { sides: [WindowInsetsSides::NONE; KINDS] };

    /// The status bar, as `WindowInsets.statusBars`.
    pub const fn status_bars() -> WindowInsets {
        WindowInsets::of(STATUS_BARS)
    }

    /// The navigation bar (or gesture area's bar), as `WindowInsets.navigationBars`.
    pub const fn navigation_bars() -> WindowInsets {
        WindowInsets::of(NAVIGATION_BARS)
    }

    /// A freeform window's caption bar, as `WindowInsets.captionBar`.
    pub const fn caption_bar() -> WindowInsets {
        WindowInsets::of(CAPTION_BAR)
    }

    /// The soft keyboard, as `WindowInsets.ime`.
    pub const fn ime() -> WindowInsets {
        WindowInsets::of(IME)
    }

    /// A display cutout (a camera notch), as `WindowInsets.displayCutout`.
    pub const fn display_cutout() -> WindowInsets {
        WindowInsets::of(DISPLAY_CUTOUT)
    }

    /// The status, navigation and caption bars, as `WindowInsets.systemBars`.
    pub const fn system_bars() -> WindowInsets {
        WindowInsets::status_bars()
            .union(WindowInsets::navigation_bars())
            .union(WindowInsets::caption_bar())
    }

    /// What content must not draw under: the system bars, the keyboard and a cutout, as
    /// `WindowInsets.safeDrawing`.
    pub const fn safe_drawing() -> WindowInsets {
        WindowInsets::system_bars().union(WindowInsets::ime()).union(WindowInsets::display_cutout())
    }

    /// These insets and `other`'s: the larger at each side, as `WindowInsets.union`.
    pub const fn union(self, other: WindowInsets) -> WindowInsets {
        let mut sides = self.sides;
        let mut kind = 0;
        while kind < KINDS {
            sides[kind] = WindowInsetsSides(sides[kind].0 | other.sides[kind].0);
            kind += 1;
        }
        WindowInsets { sides }
    }

    /// These insets at `sides` only (zero elsewhere), as `WindowInsets.only`.
    pub const fn only(self, sides: WindowInsetsSides) -> WindowInsets {
        let mut kept = self.sides;
        let mut kind = 0;
        while kind < KINDS {
            kept[kind] = WindowInsetsSides(kept[kind].0 & sides.0);
            kind += 1;
        }
        WindowInsets { sides: kept }
    }

    /// Their values in pixels, given the platform's.
    pub fn resolve(&self, platform: &PlatformWindowInsets) -> Insets {
        let values = [
            platform.status_bars,
            platform.navigation_bars,
            platform.caption_bar,
            platform.ime,
            platform.display_cutout,
        ];
        let mut insets = Insets::ZERO;
        for (sides, value) in self.sides.iter().zip(values) {
            let pick = |side: WindowInsetsSides, value: i32| {
                if sides.contains(side) { value.max(0) } else { 0 }
            };
            insets = insets.union(Insets::new(
                pick(WindowInsetsSides::LEFT, value.left),
                pick(WindowInsetsSides::TOP, value.top),
                pick(WindowInsetsSides::RIGHT, value.right),
                pick(WindowInsetsSides::BOTTOM, value.bottom),
            ));
        }
        insets
    }

    /// Their values as paddings (start and end by the layout direction), as Compose's
    /// `WindowInsets.asPaddingValues()`: read in composition, which recomposes when they change.
    pub fn as_padding_values(&self) -> PaddingValues {
        let insets = self.resolve(&LOCAL_PLATFORM_WINDOW_INSETS.current());
        let density = LOCAL_DENSITY.current();
        let dp = |pixels: i32| Dp(pixels as f32 / density.density);
        let (start, end) = match LOCAL_LAYOUT_DIRECTION.current() {
            LayoutDirection::Ltr => (insets.left, insets.right),
            LayoutDirection::Rtl => (insets.right, insets.left),
        };
        PaddingValues {
            start: dp(start),
            top: dp(insets.top),
            end: dp(end),
            bottom: dp(insets.bottom),
        }
    }
}

/// The insets paddings, on `Modifier`, as Compose's `windowInsetsPadding` and its shorthands.
/// They don't consume what they pad (Compose's do: nested paddings of the same insets add up
/// here, D222).
pub trait WindowInsetsModifiers {
    /// Pads the content by `insets`, as `Modifier.windowInsetsPadding(insets)`.
    fn window_insets_padding(self, insets: WindowInsets) -> Modifier;
    /// As `Modifier.systemBarsPadding()`.
    fn system_bars_padding(self) -> Modifier;
    /// As `Modifier.statusBarsPadding()`.
    fn status_bars_padding(self) -> Modifier;
    /// As `Modifier.navigationBarsPadding()`.
    fn navigation_bars_padding(self) -> Modifier;
    /// As `Modifier.imePadding()`.
    fn ime_padding(self) -> Modifier;
    /// As `Modifier.displayCutoutPadding()`.
    fn display_cutout_padding(self) -> Modifier;
    /// As `Modifier.safeDrawingPadding()`.
    fn safe_drawing_padding(self) -> Modifier;
}

impl WindowInsetsModifiers for Modifier {
    fn window_insets_padding(self, insets: WindowInsets) -> Modifier {
        self.then_custom(InsetsPadding(insets))
    }
    fn system_bars_padding(self) -> Modifier {
        self.window_insets_padding(WindowInsets::system_bars())
    }
    fn status_bars_padding(self) -> Modifier {
        self.window_insets_padding(WindowInsets::status_bars())
    }
    fn navigation_bars_padding(self) -> Modifier {
        self.window_insets_padding(WindowInsets::navigation_bars())
    }
    fn ime_padding(self) -> Modifier {
        self.window_insets_padding(WindowInsets::ime())
    }
    fn display_cutout_padding(self) -> Modifier {
        self.window_insets_padding(WindowInsets::display_cutout())
    }
    fn safe_drawing_padding(self) -> Modifier {
        self.window_insets_padding(WindowInsets::safe_drawing())
    }
}

/// The element and the node of `window_insets_padding`.
#[derive(Clone, Copy, PartialEq)]
struct InsetsPadding(WindowInsets);

impl ModifierNodeElement for InsetsPadding {
    fn create(&self) -> Box<dyn ModifierNode> {
        Box::new(*self)
    }
    fn update(&self, node: &mut dyn ModifierNode) {
        if let Some(node) = node.downcast_mut::<InsetsPadding>() {
            *node = *self;
        }
    }
    fn same(&self, other: &dyn ModifierNodeElement) -> bool {
        other.downcast_ref::<InsetsPadding>().is_some_and(|other| other == self)
    }
    fn name(&self) -> &'static str {
        "window_insets_padding"
    }
}

impl ModifierNode for InsetsPadding {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for InsetsPadding {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        // A read of the host's state: a change of insets measures this node again.
        let insets = self.0.resolve(&scope.window_insets());
        let horizontal = insets.left + insets.right;
        let vertical = insets.top + insets.bottom;
        let placeable = scope.measure(content, constraints.offset(-horizontal, -vertical));
        let width = constraints.constrain_width(placeable.width + horizontal);
        let height = constraints.constrain_height(placeable.height + vertical);
        scope.place(&placeable, insets.left, insets.top);
        scope.layout(width, height)
    }
}
