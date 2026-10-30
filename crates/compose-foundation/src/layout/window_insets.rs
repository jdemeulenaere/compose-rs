//! Window insets, as Compose's `androidx.compose.foundation.layout.WindowInsets`: the space the
//! system draws over at the window's edges (status and navigation bars, the soft keyboard, a
//! display cutout, the gesture areas), which the host reports (`UiHost::set_window_insets`;
//! zero on desktop), and the modifiers that keep content clear of it.
//!
//! A [`WindowInsets`] is a value, as Compose's: fixed sizes, the system's insets of a type
//! (`WindowInsets::status_bars()`, which keeps the host's insets and reads them where it's
//! used), or what `union`, `exclude`, `add` and `only` make of others. Reading one while
//! measuring is observed: an insets change measures again what read it, nothing recomposes.
//!
//! The paddings consume what they pad, as Compose's (D1355): `window_insets_padding`,
//! `consume_window_insets` and the size modifiers find the insets their ancestors consumed
//! (`Modifier.on_consumed_window_insets_changed` hears them), so nested paddings of the same
//! insets pad once.

use std::any::Any;
use std::cell::Cell;
use std::fmt;
use std::rc::Rc;

use compose_runtime::{Callback, IntoCallback, OwnedState, mutable_state_of};
use compose_ui::platform::{
    Action, Insets, InsetsType, LOCAL_WINDOW_INSETS_SOURCE, WindowInsetsSource,
};
use compose_ui::{
    Constraints, Density, Dp, InspectorInfo, IntSize, IntrinsicMeasurable, IntrinsicMeasureScope,
    LOCAL_DENSITY, LOCAL_LAYOUT_DIRECTION, LayoutCoordinates, LayoutDirection, LayoutModifierNode,
    Measurable, MeasureResult, MeasureScope, Modifier, ModifierNode, ModifierNodeElement,
    NodeContext, NodeHandle, PaddingValues, TraversableNode, TraverseDescendantsAction,
    TraverseKey,
};

// ---- Sides --------------------------------------------------------------------------------------

const ALLOW_LEFT_IN_LTR: u8 = 1 << 3;
const ALLOW_RIGHT_IN_LTR: u8 = 1 << 2;
const ALLOW_LEFT_IN_RTL: u8 = 1 << 1;
const ALLOW_RIGHT_IN_RTL: u8 = 1;

/// Sides of the window, as Compose's `WindowInsetsSides`: combine them with `+` (or `|`).
/// `START` and `END` follow the layout direction; `LEFT` and `RIGHT` don't.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct WindowInsetsSides(u8);

impl WindowInsetsSides {
    pub const START: WindowInsetsSides = WindowInsetsSides(ALLOW_LEFT_IN_LTR | ALLOW_RIGHT_IN_RTL);
    pub const END: WindowInsetsSides = WindowInsetsSides(ALLOW_RIGHT_IN_LTR | ALLOW_LEFT_IN_RTL);
    pub const TOP: WindowInsetsSides = WindowInsetsSides(1 << 4);
    pub const BOTTOM: WindowInsetsSides = WindowInsetsSides(1 << 5);
    pub const LEFT: WindowInsetsSides = WindowInsetsSides(ALLOW_LEFT_IN_LTR | ALLOW_LEFT_IN_RTL);
    pub const RIGHT: WindowInsetsSides = WindowInsetsSides(ALLOW_RIGHT_IN_LTR | ALLOW_RIGHT_IN_RTL);
    pub const HORIZONTAL: WindowInsetsSides =
        WindowInsetsSides::LEFT.plus(WindowInsetsSides::RIGHT);
    pub const VERTICAL: WindowInsetsSides = WindowInsetsSides::TOP.plus(WindowInsetsSides::BOTTOM);

    /// These sides and `sides`, as `WindowInsetsSides.plus`.
    pub const fn plus(self, sides: WindowInsetsSides) -> WindowInsetsSides {
        WindowInsetsSides(self.0 | sides.0)
    }

    const fn has_any(self, sides: u8) -> bool {
        self.0 & sides != 0
    }
}

impl std::ops::Add for WindowInsetsSides {
    type Output = WindowInsetsSides;
    fn add(self, sides: WindowInsetsSides) -> WindowInsetsSides {
        self.plus(sides)
    }
}

impl std::ops::BitOr for WindowInsetsSides {
    type Output = WindowInsetsSides;
    fn bitor(self, sides: WindowInsetsSides) -> WindowInsetsSides {
        self.plus(sides)
    }
}

impl fmt::Display for WindowInsetsSides {
    /// As Compose's `toString`: `WindowInsetsSides(Start+Top+Bottom)`.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WindowInsetsSides(")?;
        let named = [
            (WindowInsetsSides::START, "Start"),
            (WindowInsetsSides::LEFT, "Left"),
            (WindowInsetsSides::TOP, "Top"),
            (WindowInsetsSides::END, "End"),
            (WindowInsetsSides::RIGHT, "Right"),
            (WindowInsetsSides::BOTTOM, "Bottom"),
        ];
        let mut first = true;
        for (sides, name) in named {
            if self.0 & sides.0 == sides.0 {
                if !first {
                    formatter.write_str("+")?;
                }
                formatter.write_str(name)?;
                first = false;
            }
        }
        formatter.write_str(")")
    }
}

impl fmt::Debug for WindowInsetsSides {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

// ---- Insets -------------------------------------------------------------------------------------

/// Insets at the window's edges, as Compose's `WindowInsets`: a value whose sizes are read
/// where it's used (`get_left`, `get_top`, …, given the density and the layout direction).
///
/// Made of fixed sizes ([`new`](Self::new), in pixels or `Dp`), the system's insets of a type
/// (`status_bars()`, `ime()`, `safe_drawing()`, …, read in composition: they keep the host's
/// insets, Compose's `WindowInsetsHolder`), a [`MutableWindowInsets`], your own
/// ([`from_provider`](Self::from_provider)), or what [`union`](Self::union),
/// [`exclude`](Self::exclude), [`add`](Self::add) and [`only`](Self::only) make of others.
/// Equal as Compose's are: by what they're made of, the system's by their type.
#[derive(Clone)]
pub struct WindowInsets(Rc<Inner>);

enum Inner {
    Fixed([i32; 4]),
    FixedDp([Dp; 4]),
    System(WindowInsetsSource, SystemKind),
    Union(WindowInsets, WindowInsets),
    Add(WindowInsets, WindowInsets),
    Exclude(WindowInsets, WindowInsets),
    Only(WindowInsets, WindowInsetsSides),
    Padding(PaddingValues),
    Mutable(MutableWindowInsets),
    /// What `recalculate_window_insets` consumes: the space around it, or (while it doesn't
    /// know it) what its ancestors consumed.
    Recalculated(ValueInsets, WindowInsets),
    Provider(Rc<dyn WindowInsetsProvider>),
}

/// Which of the system's insets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SystemKind {
    Type(InsetsType),
    IgnoringVisibility(InsetsType),
    ImeAnimationSource,
    ImeAnimationTarget,
}

impl SystemKind {
    fn name(self) -> &'static str {
        use InsetsType::*;
        match self {
            SystemKind::Type(kind) => match kind {
                StatusBars => "statusBars",
                NavigationBars => "navigationBars",
                CaptionBar => "captionBar",
                Ime => "ime",
                DisplayCutout => "displayCutout",
                SystemBars => "systemBars",
                SystemGestures => "systemGestures",
                MandatorySystemGestures => "mandatorySystemGestures",
                TappableElement => "tappableElement",
                Waterfall => "waterfall",
            },
            SystemKind::IgnoringVisibility(kind) => match kind {
                StatusBars => "statusBarsIgnoringVisibility",
                NavigationBars => "navigationBarsIgnoringVisibility",
                CaptionBar => "captionBarIgnoringVisibility",
                SystemBars => "systemBarsIgnoringVisibility",
                _ => "tappableElementIgnoringVisibility",
            },
            SystemKind::ImeAnimationSource => "imeAnimationSource",
            SystemKind::ImeAnimationTarget => "imeAnimationTarget",
        }
    }

    fn resolve(self, source: &WindowInsetsSource) -> Insets {
        match self {
            SystemKind::Type(kind) => source.insets(kind),
            SystemKind::IgnoringVisibility(kind) => source.insets_ignoring_visibility(kind),
            SystemKind::ImeAnimationSource => source.ime_animation_source(),
            SystemKind::ImeAnimationTarget => source.ime_animation_target(),
        }
    }
}

/// Insets you compute, as an implementation of Compose's `WindowInsets` interface: what
/// [`WindowInsets::from_provider`] wraps. Shown (`Display`) in the insets' `to_string`.
pub trait WindowInsetsProvider: Any + fmt::Display {
    fn get_left(&self, density: &Density, layout_direction: LayoutDirection) -> i32;
    fn get_top(&self, density: &Density) -> i32;
    fn get_right(&self, density: &Density, layout_direction: LayoutDirection) -> i32;
    fn get_bottom(&self, density: &Density) -> i32;
    /// Whether `other` is the same insets (Compose's `equals`). By default, only this one.
    fn same(&self, other: &dyn Any) -> bool {
        std::ptr::addr_eq(self as *const Self, other as *const dyn Any)
    }
}

/// The units fixed insets take, as `WindowInsets`' `Int` (pixels) and `Dp` overloads.
pub trait InsetsLength: Copy + sealed::Sealed {
    #[doc(hidden)]
    fn insets(left: Self, top: Self, right: Self, bottom: Self) -> WindowInsets;
}

mod sealed {
    #[allow(unnameable_types)]
    pub trait Sealed {}
    impl Sealed for i32 {}
    impl Sealed for compose_ui::Dp {}
}

impl InsetsLength for i32 {
    fn insets(left: i32, top: i32, right: i32, bottom: i32) -> WindowInsets {
        WindowInsets::of(Inner::Fixed([left, top, right, bottom]))
    }
}

impl InsetsLength for Dp {
    fn insets(left: Dp, top: Dp, right: Dp, bottom: Dp) -> WindowInsets {
        WindowInsets::of(Inner::FixedDp([left, top, right, bottom]))
    }
}

impl Default for WindowInsets {
    /// No insets, as `WindowInsets()`.
    fn default() -> WindowInsets {
        WindowInsets::new(0, 0, 0, 0)
    }
}

impl WindowInsets {
    fn of(inner: Inner) -> WindowInsets {
        WindowInsets(Rc::new(inner))
    }

    /// Fixed insets, in pixels (`WindowInsets::new(10, 0, 10, 0)`) or in `Dp`
    /// (`WindowInsets::new(8.dp(), …)`, rounded to pixels where read), as `WindowInsets(left,
    /// top, right, bottom)`.
    pub fn new<T: InsetsLength>(left: T, top: T, right: T, bottom: T) -> WindowInsets {
        T::insets(left, top, right, bottom)
    }

    /// Your own insets (Compose's `WindowInsets` interface, implemented).
    pub fn from_provider(provider: impl WindowInsetsProvider) -> WindowInsets {
        WindowInsets::of(Inner::Provider(Rc::new(provider)))
    }

    /// The left inset in pixels, as `getLeft(density, layoutDirection)`.
    pub fn get_left(&self, density: &Density, layout_direction: LayoutDirection) -> i32 {
        self.side(Side::Left, density, layout_direction)
    }

    /// The top inset in pixels, as `getTop(density)`.
    pub fn get_top(&self, density: &Density) -> i32 {
        self.side(Side::Top, density, LayoutDirection::Ltr)
    }

    /// The right inset in pixels, as `getRight(density, layoutDirection)`.
    pub fn get_right(&self, density: &Density, layout_direction: LayoutDirection) -> i32 {
        self.side(Side::Right, density, layout_direction)
    }

    /// The bottom inset in pixels, as `getBottom(density)`.
    pub fn get_bottom(&self, density: &Density) -> i32 {
        self.side(Side::Bottom, density, LayoutDirection::Ltr)
    }

    fn side(&self, side: Side, density: &Density, direction: LayoutDirection) -> i32 {
        let of = |insets: &WindowInsets| insets.side(side, density, direction);
        match &*self.0 {
            Inner::Fixed(values) => values[side as usize],
            Inner::FixedDp(values) => density.round_to_px(values[side as usize]),
            Inner::System(source, kind) => side.of(kind.resolve(source)),
            Inner::Union(first, second) => of(first).max(of(second)),
            Inner::Add(first, second) => of(first) + of(second),
            Inner::Exclude(included, excluded) => (of(included) - of(excluded)).max(0),
            Inner::Only(insets, sides) => {
                let allowed = match (side, direction) {
                    (Side::Left, LayoutDirection::Ltr) => ALLOW_LEFT_IN_LTR,
                    (Side::Left, LayoutDirection::Rtl) => ALLOW_LEFT_IN_RTL,
                    (Side::Right, LayoutDirection::Ltr) => ALLOW_RIGHT_IN_LTR,
                    (Side::Right, LayoutDirection::Rtl) => ALLOW_RIGHT_IN_RTL,
                    (Side::Top, _) => WindowInsetsSides::TOP.0,
                    (Side::Bottom, _) => WindowInsetsSides::BOTTOM.0,
                };
                if sides.has_any(allowed) { of(insets) } else { 0 }
            }
            Inner::Padding(padding) => {
                let dp =
                    match (side, direction) {
                        (Side::Left, LayoutDirection::Ltr)
                        | (Side::Right, LayoutDirection::Rtl) => padding.start,
                        (Side::Left, LayoutDirection::Rtl)
                        | (Side::Right, LayoutDirection::Ltr) => padding.end,
                        (Side::Top, _) => padding.top,
                        (Side::Bottom, _) => padding.bottom,
                    };
                density.round_to_px(dp)
            }
            Inner::Mutable(mutable) => of(&mutable.insets()),
            Inner::Recalculated(value, ancestor) => match value.get() {
                Some(values) => side.of(values),
                None => of(ancestor),
            },
            Inner::Provider(provider) => match side {
                Side::Left => provider.get_left(density, direction),
                Side::Top => provider.get_top(density),
                Side::Right => provider.get_right(density, direction),
                Side::Bottom => provider.get_bottom(density),
            },
        }
    }

    /// The larger of these and `insets` at each side, as `WindowInsets.union`.
    pub fn union(self, insets: WindowInsets) -> WindowInsets {
        WindowInsets::of(Inner::Union(self, insets))
    }

    /// These less `insets` at each side, never below zero, as `WindowInsets.exclude`.
    pub fn exclude(self, insets: WindowInsets) -> WindowInsets {
        WindowInsets::of(Inner::Exclude(self, insets))
    }

    /// These and `insets` added at each side, as `WindowInsets.add` (Compose's name; it has no
    /// operator).
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, insets: WindowInsets) -> WindowInsets {
        WindowInsets::of(Inner::Add(self, insets))
    }

    /// These at `sides` only (zero elsewhere), as `WindowInsets.only`.
    pub fn only(self, sides: WindowInsetsSides) -> WindowInsets {
        WindowInsets::of(Inner::Only(self, sides))
    }

    /// Their sizes as paddings, with the composition's density and layout direction, as
    /// `asPaddingValues()`. Read now, in composition, which recomposes when they change
    /// (Compose's are read where the padding is: compose-rs's `PaddingValues` is a value).
    pub fn as_padding_values(&self) -> PaddingValues {
        self.as_padding_values_with(&LOCAL_DENSITY.current(), LOCAL_LAYOUT_DIRECTION.current())
    }

    /// Their sizes as paddings at `density`, start and end by `layout_direction`, as
    /// `asPaddingValues(density)`.
    pub fn as_padding_values_with(
        &self,
        density: &Density,
        layout_direction: LayoutDirection,
    ) -> PaddingValues {
        let dp = |pixels: i32| Dp(pixels as f32 / density.density);
        let (left, right) =
            (self.get_left(density, layout_direction), self.get_right(density, layout_direction));
        let (start, end) = match layout_direction {
            LayoutDirection::Ltr => (left, right),
            LayoutDirection::Rtl => (right, left),
        };
        PaddingValues {
            start: dp(start),
            top: dp(self.get_top(density)),
            end: dp(end),
            bottom: dp(self.get_bottom(density)),
        }
    }
}

impl PartialEq for WindowInsets {
    fn eq(&self, other: &WindowInsets) -> bool {
        if Rc::ptr_eq(&self.0, &other.0) {
            return true;
        }
        match (&*self.0, &*other.0) {
            (Inner::Fixed(a), Inner::Fixed(b)) => a == b,
            (Inner::FixedDp(a), Inner::FixedDp(b)) => a == b,
            (Inner::System(a, kind_a), Inner::System(b, kind_b)) => a == b && kind_a == kind_b,
            (Inner::Union(a, b), Inner::Union(c, d))
            | (Inner::Add(a, b), Inner::Add(c, d))
            | (Inner::Exclude(a, b), Inner::Exclude(c, d)) => a == c && b == d,
            (Inner::Only(a, sides_a), Inner::Only(b, sides_b)) => a == b && sides_a == sides_b,
            (Inner::Padding(a), Inner::Padding(b)) => a == b,
            (Inner::Mutable(a), Inner::Mutable(b)) => a == b,
            (Inner::Recalculated(a, ancestor_a), Inner::Recalculated(b, ancestor_b)) => {
                a == b && ancestor_a == ancestor_b
            }
            (Inner::Provider(a), Inner::Provider(b)) => a.same(&**b as &dyn Any),
            _ => false,
        }
    }
}

impl compose_runtime::ComposeArg for WindowInsets {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl fmt::Display for WindowInsets {
    /// As Compose's `toString`s: `Insets(left=10, top=11, right=12, bottom=13)`,
    /// `statusBars(0, 63, 0, 0)`, `(… ∪ …)`, `(… only WindowInsetsSides(Top))`, ….
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &*self.0 {
            Inner::Fixed([left, top, right, bottom]) => {
                write!(formatter, "Insets(left={left}, top={top}, right={right}, bottom={bottom})")
            }
            Inner::FixedDp([left, top, right, bottom]) => write!(
                formatter,
                "Insets(left={:?}.dp, top={:?}.dp, right={:?}.dp, bottom={:?}.dp)",
                left.0, top.0, right.0, bottom.0
            ),
            Inner::System(source, kind) => {
                let Insets { left, top, right, bottom } = kind.resolve(source);
                match kind {
                    SystemKind::Type(kind) if *kind != InsetsType::Waterfall => {
                        let name = SystemKind::Type(*kind).name();
                        write!(formatter, "{name}({left}, {top}, {right}, {bottom})")
                    }
                    _ => write!(
                        formatter,
                        "{}(left={left}, top={top}, right={right}, bottom={bottom})",
                        kind.name()
                    ),
                }
            }
            Inner::Union(first, second) => write!(formatter, "({first} ∪ {second})"),
            Inner::Add(first, second) => write!(formatter, "({first} + {second})"),
            Inner::Exclude(included, excluded) => write!(formatter, "({included} - {excluded})"),
            Inner::Only(insets, sides) => write!(formatter, "({insets} only {sides})"),
            Inner::Padding(padding) => write!(
                formatter,
                "PaddingValues({:?}, {:?}, {:?}, {:?})",
                padding.start, padding.top, padding.end, padding.bottom
            ),
            Inner::Mutable(mutable) => {
                write!(formatter, "MutableWindowInsets({})", mutable.insets())
            }
            Inner::Recalculated(value, ancestor) => match value.get() {
                Some(Insets { left, top, right, bottom }) => write!(
                    formatter,
                    "reset(left={left}, top={top}, right={right}, bottom={bottom})"
                ),
                None => write!(formatter, "{ancestor}"),
            },
            Inner::Provider(provider) => write!(formatter, "{provider}"),
        }
    }
}

impl fmt::Debug for WindowInsets {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

#[derive(Clone, Copy)]
enum Side {
    Left,
    Top,
    Right,
    Bottom,
}

impl Side {
    fn of(self, insets: Insets) -> i32 {
        match self {
            Side::Left => insets.left,
            Side::Top => insets.top,
            Side::Right => insets.right,
            Side::Bottom => insets.bottom,
        }
    }
}

// ---- The system's insets ------------------------------------------------------------------------

/// The system's insets of a host, by type: Compose's `WindowInsetsHolder`.
#[derive(Clone)]
struct Holder(WindowInsetsSource);

impl Holder {
    /// The composition's host's.
    fn current() -> Holder {
        Holder(LOCAL_WINDOW_INSETS_SOURCE.current())
    }

    fn kind(&self, kind: InsetsType) -> WindowInsets {
        WindowInsets::of(Inner::System(self.0.clone(), SystemKind::Type(kind)))
    }

    fn ignoring_visibility(&self, kind: InsetsType) -> WindowInsets {
        WindowInsets::of(Inner::System(self.0.clone(), SystemKind::IgnoringVisibility(kind)))
    }

    fn system_bars(&self) -> WindowInsets {
        self.kind(InsetsType::SystemBars)
    }

    fn safe_drawing(&self) -> WindowInsets {
        self.system_bars()
            .union(self.kind(InsetsType::Ime))
            .union(self.kind(InsetsType::DisplayCutout))
    }

    fn safe_gestures(&self) -> WindowInsets {
        self.kind(InsetsType::TappableElement)
            .union(self.kind(InsetsType::MandatorySystemGestures))
            .union(self.kind(InsetsType::SystemGestures))
            .union(self.kind(InsetsType::Waterfall))
    }

    fn safe_content(&self) -> WindowInsets {
        self.safe_drawing().union(self.safe_gestures())
    }

    fn visible(&self, kind: InsetsType) -> bool {
        self.0.is_visible(kind)
    }
}

/// The system's insets, read in composition (Compose's `@Composable` getters): they keep the
/// composition's host's insets, read where they're used.
impl WindowInsets {
    /// The caption bar of a freeform window, as `WindowInsets.captionBar`.
    pub fn caption_bar() -> WindowInsets {
        Holder::current().kind(InsetsType::CaptionBar)
    }

    /// A display cutout (a camera notch), as `WindowInsets.displayCutout`.
    pub fn display_cutout() -> WindowInsets {
        Holder::current().kind(InsetsType::DisplayCutout)
    }

    /// The soft keyboard, as `WindowInsets.ime`.
    pub fn ime() -> WindowInsets {
        Holder::current().kind(InsetsType::Ime)
    }

    /// Where the system's gestures always win over the app's, as
    /// `WindowInsets.mandatorySystemGestures`.
    pub fn mandatory_system_gestures() -> WindowInsets {
        Holder::current().kind(InsetsType::MandatorySystemGestures)
    }

    /// The navigation bar (or the gesture area's handle), as `WindowInsets.navigationBars`.
    pub fn navigation_bars() -> WindowInsets {
        Holder::current().kind(InsetsType::NavigationBars)
    }

    /// The status bar, as `WindowInsets.statusBars`.
    pub fn status_bars() -> WindowInsets {
        Holder::current().kind(InsetsType::StatusBars)
    }

    /// The status, navigation and caption bars, as `WindowInsets.systemBars`.
    pub fn system_bars() -> WindowInsets {
        Holder::current().system_bars()
    }

    /// Where the system's gestures may win over the app's, as `WindowInsets.systemGestures`.
    pub fn system_gestures() -> WindowInsets {
        Holder::current().kind(InsetsType::SystemGestures)
    }

    /// Where the system's bars take taps, as `WindowInsets.tappableElement`.
    pub fn tappable_element() -> WindowInsets {
        Holder::current().kind(InsetsType::TappableElement)
    }

    /// A waterfall display's curved edges, as `WindowInsets.waterfall`.
    pub fn waterfall() -> WindowInsets {
        Holder::current().kind(InsetsType::Waterfall)
    }

    /// What content must not draw under: the system bars, the keyboard and a cutout, as
    /// `WindowInsets.safeDrawing`.
    pub fn safe_drawing() -> WindowInsets {
        Holder::current().safe_drawing()
    }

    /// Where gestures might go to the system: the tappable elements, the gesture areas and the
    /// waterfall, as `WindowInsets.safeGestures`.
    pub fn safe_gestures() -> WindowInsets {
        Holder::current().safe_gestures()
    }

    /// `safe_drawing` and `safe_gestures`, as `WindowInsets.safeContent`.
    pub fn safe_content() -> WindowInsets {
        Holder::current().safe_content()
    }

    /// The caption bar's insets were it shown, as `WindowInsets.captionBarIgnoringVisibility`.
    pub fn caption_bar_ignoring_visibility() -> WindowInsets {
        Holder::current().ignoring_visibility(InsetsType::CaptionBar)
    }

    /// As `WindowInsets.navigationBarsIgnoringVisibility`.
    pub fn navigation_bars_ignoring_visibility() -> WindowInsets {
        Holder::current().ignoring_visibility(InsetsType::NavigationBars)
    }

    /// As `WindowInsets.statusBarsIgnoringVisibility`.
    pub fn status_bars_ignoring_visibility() -> WindowInsets {
        Holder::current().ignoring_visibility(InsetsType::StatusBars)
    }

    /// As `WindowInsets.systemBarsIgnoringVisibility`.
    pub fn system_bars_ignoring_visibility() -> WindowInsets {
        Holder::current().ignoring_visibility(InsetsType::SystemBars)
    }

    /// As `WindowInsets.tappableElementIgnoringVisibility`.
    pub fn tappable_element_ignoring_visibility() -> WindowInsets {
        Holder::current().ignoring_visibility(InsetsType::TappableElement)
    }

    /// The keyboard's insets where its animation started (its insets when none runs), as
    /// `WindowInsets.imeAnimationSource`.
    pub fn ime_animation_source() -> WindowInsets {
        WindowInsets::of(Inner::System(Holder::current().0, SystemKind::ImeAnimationSource))
    }

    /// Where the keyboard's animation will end, as `WindowInsets.imeAnimationTarget`.
    pub fn ime_animation_target() -> WindowInsets {
        WindowInsets::of(Inner::System(Holder::current().0, SystemKind::ImeAnimationTarget))
    }

    /// The display cutout's outline, if the platform reports one (Android 12 and later), as
    /// `WindowInsets.cutoutPath`: read in composition, which recomposes when it changes.
    pub fn cutout_path() -> Option<compose_ui::Path> {
        Holder::current().0.cutout_path()
    }

    /// Whether the caption bar is shown, as `WindowInsets.isCaptionBarVisible`: read in
    /// composition, which recomposes when it changes.
    pub fn is_caption_bar_visible() -> bool {
        Holder::current().visible(InsetsType::CaptionBar)
    }

    /// Whether the keyboard is shown, as `WindowInsets.isImeVisible`.
    pub fn is_ime_visible() -> bool {
        Holder::current().visible(InsetsType::Ime)
    }

    /// As `WindowInsets.areStatusBarsVisible`.
    pub fn are_status_bars_visible() -> bool {
        Holder::current().visible(InsetsType::StatusBars)
    }

    /// As `WindowInsets.areNavigationBarsVisible`.
    pub fn are_navigation_bars_visible() -> bool {
        Holder::current().visible(InsetsType::NavigationBars)
    }

    /// As `WindowInsets.areSystemBarsVisible`.
    pub fn are_system_bars_visible() -> bool {
        Holder::current().visible(InsetsType::SystemBars)
    }

    /// As `WindowInsets.isTappableElementVisible`.
    pub fn is_tappable_element_visible() -> bool {
        Holder::current().visible(InsetsType::TappableElement)
    }
}

// ---- Mutable and value insets -------------------------------------------------------------------

/// Insets held as state, as Compose's `MutableWindowInsets`: what reads them (a padding while
/// measuring) reads the state, so setting new ones measures it again. A handle: clones share
/// the insets.
#[derive(Clone)]
pub struct MutableWindowInsets(Rc<OwnedState<WindowInsets>>);

impl MutableWindowInsets {
    /// Holding `initial`, as `MutableWindowInsets(initialInsets)`.
    pub fn new(initial: WindowInsets) -> MutableWindowInsets {
        MutableWindowInsets(Rc::new(mutable_state_of(initial)))
    }

    /// The insets held: a read of the state.
    pub fn insets(&self) -> WindowInsets {
        self.0.get()
    }

    /// Holds `insets` from now on, as `insets = …`.
    pub fn set_insets(&self, insets: WindowInsets) {
        self.0.set(insets);
    }

    /// As insets: the ones held, wherever they're read.
    pub fn as_window_insets(&self) -> WindowInsets {
        WindowInsets::of(Inner::Mutable(self.clone()))
    }

    /// As [`WindowInsets::get_left`], of the insets held.
    pub fn get_left(&self, density: &Density, layout_direction: LayoutDirection) -> i32 {
        self.insets().get_left(density, layout_direction)
    }

    /// As [`WindowInsets::get_top`].
    pub fn get_top(&self, density: &Density) -> i32 {
        self.insets().get_top(density)
    }

    /// As [`WindowInsets::get_right`].
    pub fn get_right(&self, density: &Density, layout_direction: LayoutDirection) -> i32 {
        self.insets().get_right(density, layout_direction)
    }

    /// As [`WindowInsets::get_bottom`].
    pub fn get_bottom(&self, density: &Density) -> i32 {
        self.insets().get_bottom(density)
    }
}

impl Default for MutableWindowInsets {
    /// Holding no insets, as `MutableWindowInsets()`.
    fn default() -> MutableWindowInsets {
        MutableWindowInsets::new(WindowInsets::default())
    }
}

impl PartialEq for MutableWindowInsets {
    /// The same holder.
    fn eq(&self, other: &MutableWindowInsets) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl compose_runtime::ComposeArg for MutableWindowInsets {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl From<MutableWindowInsets> for WindowInsets {
    fn from(mutable: MutableWindowInsets) -> WindowInsets {
        mutable.as_window_insets()
    }
}

impl From<&MutableWindowInsets> for WindowInsets {
    fn from(mutable: &MutableWindowInsets) -> WindowInsets {
        mutable.as_window_insets()
    }
}

/// Pixel insets held as state, as Compose's `ValueInsets`; `None` stands for its `-1`s (not
/// known).
#[derive(Clone)]
struct ValueInsets(Rc<OwnedState<Option<Insets>>>);

impl ValueInsets {
    fn new(value: Option<Insets>) -> ValueInsets {
        ValueInsets(Rc::new(mutable_state_of(value)))
    }

    fn get(&self) -> Option<Insets> {
        self.0.get()
    }

    fn set(&self, value: Option<Insets>) {
        self.0.set(value);
    }
}

impl PartialEq for ValueInsets {
    fn eq(&self, other: &ValueInsets) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

// ---- Modifiers ----------------------------------------------------------------------------------

/// What `consume_window_insets` takes: insets (consumed as their union with what's consumed
/// already) or paddings (added to it), as Compose's two overloads.
#[derive(Clone)]
pub enum ConsumedInsets {
    Insets(WindowInsets),
    Padding(PaddingValues),
}

impl From<WindowInsets> for ConsumedInsets {
    fn from(insets: WindowInsets) -> ConsumedInsets {
        ConsumedInsets::Insets(insets)
    }
}

impl From<PaddingValues> for ConsumedInsets {
    fn from(padding: PaddingValues) -> ConsumedInsets {
        ConsumedInsets::Padding(padding)
    }
}

/// The insets modifiers, on `Modifier`, as Compose's `windowInsetsPadding`, its shorthands,
/// the consumption modifiers and the size modifiers.
pub trait WindowInsetsModifiers {
    /// Pads the content by `insets`, less what its ancestors consumed, and consumes them for
    /// its content, as `Modifier.windowInsetsPadding(insets)`.
    fn window_insets_padding(self, insets: WindowInsets) -> Modifier;
    /// Consumes `insets` (or paddings) for the content, without padding it, as
    /// `Modifier.consumeWindowInsets`: the content's insets paddings pad less by them.
    fn consume_window_insets(self, insets: impl Into<ConsumedInsets>) -> Modifier;
    /// Calls `block` with the insets the ancestors consumed when it's attached and when they
    /// change, as `Modifier.onConsumedWindowInsetsChanged`.
    fn on_consumed_window_insets_changed(
        self,
        block: impl IntoCallback<dyn Fn(WindowInsets)>,
    ) -> Modifier;
    /// As `Modifier.safeDrawingPadding()`.
    fn safe_drawing_padding(self) -> Modifier;
    /// As `Modifier.safeGesturesPadding()`.
    fn safe_gestures_padding(self) -> Modifier;
    /// As `Modifier.safeContentPadding()`.
    fn safe_content_padding(self) -> Modifier;
    /// As `Modifier.systemBarsPadding()`.
    fn system_bars_padding(self) -> Modifier;
    /// As `Modifier.displayCutoutPadding()`.
    fn display_cutout_padding(self) -> Modifier;
    /// As `Modifier.statusBarsPadding()`.
    fn status_bars_padding(self) -> Modifier;
    /// As `Modifier.imePadding()`.
    fn ime_padding(self) -> Modifier;
    /// As `Modifier.navigationBarsPadding()`.
    fn navigation_bars_padding(self) -> Modifier;
    /// As `Modifier.captionBarPadding()`.
    fn caption_bar_padding(self) -> Modifier;
    /// As `Modifier.waterfallPadding()`.
    fn waterfall_padding(self) -> Modifier;
    /// As `Modifier.systemGesturesPadding()`.
    fn system_gestures_padding(self) -> Modifier;
    /// As `Modifier.mandatorySystemGesturesPadding()`.
    fn mandatory_system_gestures_padding(self) -> Modifier;
    /// Consumes the insets around the content as it lays out (the window's edges' distance
    /// from it) in place of what its ancestors consumed, as `Modifier.recalculateWindowInsets()`:
    /// for content moved or resized apart from its ancestors' paddings. Needs a fixed size;
    /// without, it passes its ancestors' consumption on.
    fn recalculate_window_insets(self) -> Modifier;
    /// As wide as `insets`' start (less what's consumed), as `Modifier.windowInsetsStartWidth`:
    /// zero wide, the content isn't measured.
    fn window_insets_start_width(self, insets: WindowInsets) -> Modifier;
    /// As `Modifier.windowInsetsEndWidth`.
    fn window_insets_end_width(self, insets: WindowInsets) -> Modifier;
    /// As high as `insets`' top (less what's consumed), as `Modifier.windowInsetsTopHeight`.
    fn window_insets_top_height(self, insets: WindowInsets) -> Modifier;
    /// As `Modifier.windowInsetsBottomHeight`.
    fn window_insets_bottom_height(self, insets: WindowInsets) -> Modifier;
}

impl WindowInsetsModifiers for Modifier {
    fn window_insets_padding(self, insets: WindowInsets) -> Modifier {
        self.then_custom(PaddingElement(insets))
    }
    fn consume_window_insets(self, insets: impl Into<ConsumedInsets>) -> Modifier {
        match insets.into() {
            ConsumedInsets::Insets(insets) => self.then_custom(ConsumeElement(insets)),
            ConsumedInsets::Padding(padding) => self.then_custom(ConsumePaddingElement(padding)),
        }
    }
    fn on_consumed_window_insets_changed(
        self,
        block: impl IntoCallback<dyn Fn(WindowInsets)>,
    ) -> Modifier {
        self.then_custom(ConsumedChangedElement(block.into_callback()))
    }
    fn safe_drawing_padding(self) -> Modifier {
        self.then_custom(SystemPadding("safeDrawingPadding", Holder::safe_drawing))
    }
    fn safe_gestures_padding(self) -> Modifier {
        self.then_custom(SystemPadding("safeGesturesPadding", Holder::safe_gestures))
    }
    fn safe_content_padding(self) -> Modifier {
        self.then_custom(SystemPadding("safeContentPadding", Holder::safe_content))
    }
    fn system_bars_padding(self) -> Modifier {
        self.then_custom(SystemPadding("systemBarsPadding", Holder::system_bars))
    }
    fn display_cutout_padding(self) -> Modifier {
        self.then_custom(SystemPadding("displayCutoutPadding", |holder: &Holder| {
            holder.kind(InsetsType::DisplayCutout)
        }))
    }
    fn status_bars_padding(self) -> Modifier {
        self.then_custom(SystemPadding("statusBarsPadding", |holder: &Holder| {
            holder.kind(InsetsType::StatusBars)
        }))
    }
    fn ime_padding(self) -> Modifier {
        self.then_custom(SystemPadding("imePadding", |holder: &Holder| {
            holder.kind(InsetsType::Ime)
        }))
    }
    fn navigation_bars_padding(self) -> Modifier {
        self.then_custom(SystemPadding("navigationBarsPadding", |holder: &Holder| {
            holder.kind(InsetsType::NavigationBars)
        }))
    }
    fn caption_bar_padding(self) -> Modifier {
        self.then_custom(SystemPadding("captionBarPadding", |holder: &Holder| {
            holder.kind(InsetsType::CaptionBar)
        }))
    }
    fn waterfall_padding(self) -> Modifier {
        self.then_custom(SystemPadding("waterfallPadding", |holder: &Holder| {
            holder.kind(InsetsType::Waterfall)
        }))
    }
    fn system_gestures_padding(self) -> Modifier {
        self.then_custom(SystemPadding("systemGesturesPadding", |holder: &Holder| {
            holder.kind(InsetsType::SystemGestures)
        }))
    }
    fn mandatory_system_gestures_padding(self) -> Modifier {
        self.then_custom(SystemPadding("mandatorySystemGesturesPadding", |holder: &Holder| {
            holder.kind(InsetsType::MandatorySystemGestures)
        }))
    }
    fn recalculate_window_insets(self) -> Modifier {
        self.then_custom(RecalculateElement)
    }
    fn window_insets_start_width(self, insets: WindowInsets) -> Modifier {
        self.then_custom(SizeElement(insets, Edge::Start))
    }
    fn window_insets_end_width(self, insets: WindowInsets) -> Modifier {
        self.then_custom(SizeElement(insets, Edge::End))
    }
    fn window_insets_top_height(self, insets: WindowInsets) -> Modifier {
        self.then_custom(SizeElement(insets, Edge::Top))
    }
    fn window_insets_bottom_height(self, insets: WindowInsets) -> Modifier {
        self.then_custom(SizeElement(insets, Edge::Bottom))
    }
}

// ---- Consumption --------------------------------------------------------------------------------

/// Which edge a size modifier takes the size of.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Edge {
    Start,
    End,
    Top,
    Bottom,
}

/// What a consuming node does with its ancestors' consumption: Compose's subclasses of
/// `InsetsConsumingModifierNode`.
enum Consumer {
    /// `windowInsetsPadding`: pads by, and consumes, its insets.
    Padding(WindowInsets),
    /// The shorthands: the system's insets of its host, known once attached.
    SystemPadding(fn(&Holder) -> WindowInsets, WindowInsets),
    /// `consumeWindowInsets(insets)`.
    Consume(WindowInsets),
    /// `consumeWindowInsets(paddingValues)`.
    ConsumePadding(PaddingValues),
    /// `onConsumedWindowInsetsChanged`.
    Changed(Callback<dyn Fn(WindowInsets)>),
    /// `recalculateWindowInsets`: the window's edges' distance from it, and the root's size.
    Recalculate(ValueInsets, Cell<IntSize>),
    /// The size modifiers: their insets, and those less what's consumed.
    Size(WindowInsets, Edge, WindowInsets),
}

thread_local! {
    static CONSUMED_INSETS_WALKS: Cell<u64> = const { Cell::new(0) };
}

/// How many times a node on this thread walked what's below it to tell it what it consumes: what
/// tests of that cost count.
#[doc(hidden)]
pub fn consumed_insets_walks() -> u64 {
    CONSUMED_INSETS_WALKS.get()
}

/// What the insets consuming nodes find each other by, as Compose's
/// `InsetsConsumingModifierNodeKey`.
static CONSUMING_NODE_KEY: TraverseKey = TraverseKey::new("ConsumedInsetsProvider");

/// The node of every insets consuming modifier, as Compose's `InsetsConsumingModifierNode`:
/// one type, so they find each other (`traverse_ancestors`, `traverse_descendants`).
struct ConsumingNode {
    consumer: Consumer,
    /// What the nearest consuming ancestor consumed, as `ancestorConsumedInsets`.
    ancestor: WindowInsets,
    /// What it consumed, for its descendants, as `consumedInsets`.
    consumed: WindowInsets,
    /// Its handle while attached: what an update finishes its work through (D1355).
    handle: Option<NodeHandle>,
}

impl ConsumingNode {
    fn new(consumer: Consumer) -> ConsumingNode {
        ConsumingNode {
            consumer,
            ancestor: WindowInsets::default(),
            consumed: WindowInsets::default(),
            handle: None,
        }
    }

    /// What it consumes, given what its ancestors did, as `calculateInsets`.
    fn calculate_insets(&self, ancestor: &WindowInsets) -> WindowInsets {
        match &self.consumer {
            Consumer::Padding(insets)
            | Consumer::SystemPadding(_, insets)
            | Consumer::Consume(insets) => ancestor.clone().union(insets.clone()),
            Consumer::ConsumePadding(padding) => {
                ancestor.clone().add(WindowInsets::of(Inner::Padding(*padding)))
            }
            Consumer::Changed(block) => {
                block(ancestor.clone());
                ancestor.clone()
            }
            Consumer::Recalculate(value, _) => {
                WindowInsets::of(Inner::Recalculated(value.clone(), ancestor.clone()))
            }
            Consumer::Size(..) => ancestor.clone(),
        }
    }

    fn is_layout(&self) -> bool {
        matches!(
            self.consumer,
            Consumer::Padding(_)
                | Consumer::SystemPadding(..)
                | Consumer::Recalculate(..)
                | Consumer::Size(..)
        )
    }

    /// Its consumption changed: computes it again and tells its descendants, as
    /// `insetsInvalidated`.
    fn insets_invalidated(&mut self, context: &mut NodeContext<'_>) {
        if !context.is_attached() {
            return;
        }
        if let Consumer::Size(insets, _, less) = &mut self.consumer {
            *less = insets.clone().exclude(self.ancestor.clone());
        }
        let consumed = self.calculate_insets(&self.ancestor);
        // What's below already holds what it consumed: nothing to tell them, the same as
        // Compose's walk, whose every `setAncestorConsumedInsets` would find no change, without
        // the walk. A scaffold's node (it consumes nothing) doesn't visit the whole screen.
        if consumed != self.consumed {
            self.consumed = consumed;
            self.invalidate_child_consumed_insets(context);
        }
        // Compose's paddings and sizes measure again; `recalculateWindowInsets` doesn't
        // (`shouldAutoInvalidate` false: what it consumes is read as state).
        if self.is_layout() && !matches!(self.consumer, Consumer::Recalculate(..)) {
            context.invalidate_measurement();
        }
    }

    /// Gives its descendants what it consumed, as `invalidateChildConsumedInsets`: the nearest
    /// consuming node down each branch.
    fn invalidate_child_consumed_insets(&self, context: &mut NodeContext<'_>) {
        CONSUMED_INSETS_WALKS.set(CONSUMED_INSETS_WALKS.get() + 1);
        let consumed = self.consumed.clone();
        context.traverse_descendants::<ConsumingNode>(|child, context| {
            child.set_ancestor_consumed_insets(consumed.clone(), context);
            TraverseDescendantsAction::SkipSubtreeAndContinueTraversal
        });
    }

    /// What the nearest consuming node above consumes, none if there's none.
    fn nearest_ancestor_consumed(context: &mut NodeContext<'_>) -> WindowInsets {
        let mut ancestor = WindowInsets::default();
        context.traverse_ancestors::<ConsumingNode>(|parent, _context| {
            ancestor = parent.consumed.clone();
            false
        });
        ancestor
    }

    /// Finds what its nearest consuming ancestor consumes, and takes it.
    fn refresh_ancestor_consumed_insets(&mut self, context: &mut NodeContext<'_>) {
        let ancestor = Self::nearest_ancestor_consumed(context);
        self.set_ancestor_consumed_insets(ancestor, context);
    }

    fn set_ancestor_consumed_insets(
        &mut self,
        ancestor: WindowInsets,
        context: &mut NodeContext<'_>,
    ) {
        if self.ancestor != ancestor {
            self.ancestor = ancestor;
            self.insets_invalidated(context);
        }
    }

    /// An update changed what it consumes: `insets_invalidated` with its context, right after
    /// the composition pass (an element's `update` runs in it, without the context), D1355.
    fn post_insets_invalidated(&self) {
        let Some(handle) = self.handle else { return };
        handle.side_effect(move || {
            handle.with_node::<ConsumingNode, _>(|node, context| node.insets_invalidated(context));
        });
    }
}

impl TraversableNode for ConsumingNode {
    fn traverse_key(&self) -> &'static TraverseKey {
        &CONSUMING_NODE_KEY
    }
}

impl ModifierNode for ConsumingNode {
    fn as_traversable(&mut self) -> Option<&mut dyn TraversableNode> {
        Some(self)
    }

    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        if self.is_layout() { Some(self) } else { None }
    }

    fn as_global_position_aware(
        &mut self,
    ) -> Option<&mut dyn compose_ui::GlobalPositionAwareModifierNode> {
        match self.consumer {
            Consumer::Recalculate(..) => Some(self),
            _ => None,
        }
    }

    fn should_auto_invalidate(&self) -> bool {
        !matches!(self.consumer, Consumer::Recalculate(..))
    }

    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.handle = Some(context.handle());
        if let Consumer::SystemPadding(getter, insets) = &mut self.consumer {
            *insets = getter(&Holder(context.window_insets_source()));
        }
        self.ancestor = Self::nearest_ancestor_consumed(context);
        self.insets_invalidated(context);
    }

    fn on_detach(&mut self, context: &mut NodeContext<'_>) {
        // Leaving: its descendants get what its ancestors consumed. Each finds its nearest
        // consuming ancestor again rather than take this node's: when a chain change removes
        // two, they leave together, out of their chain, the second still holding what the
        // first consumed (Compose detaches each while the next is still linked, D1355).
        self.consumed = self.ancestor.clone();
        context.traverse_descendants::<ConsumingNode>(|child, context| {
            child.refresh_ancestor_consumed_insets(context);
            TraverseDescendantsAction::SkipSubtreeAndContinueTraversal
        });
        self.handle = None;
    }

    fn on_reset(&mut self, _context: &mut NodeContext<'_>) {
        self.ancestor = WindowInsets::default();
    }
}

impl LayoutModifierNode for ConsumingNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let density = scope.density();
        let direction = scope.layout_direction();
        match &self.consumer {
            Consumer::Size(_, edge, less) => {
                let size = match edge {
                    Edge::Start | Edge::End => {
                        let left = (*edge == Edge::Start) == (direction == LayoutDirection::Ltr);
                        if left {
                            less.get_left(&density, direction)
                        } else {
                            less.get_right(&density, direction)
                        }
                    }
                    Edge::Top => less.get_top(&density),
                    Edge::Bottom => less.get_bottom(&density),
                };
                if size == 0 {
                    return scope.layout(0, 0);
                }
                let horizontal = matches!(edge, Edge::Start | Edge::End);
                let content_constraints = if horizontal {
                    Constraints::new(size, size, constraints.min_height, constraints.max_height)
                } else {
                    Constraints::new(constraints.min_width, constraints.max_width, size, size)
                };
                let placeable = scope.measure(content, content_constraints);
                scope.place_relative(&placeable, 0, 0);
                if horizontal {
                    scope.layout(size, placeable.height)
                } else {
                    scope.layout(placeable.width, size)
                }
            }
            Consumer::Recalculate(value, root) => {
                if !constraints.has_fixed_width() || !constraints.has_fixed_height() {
                    // It can't say what's around it: its ancestors' consumption goes on.
                    if value.get().is_some() {
                        value.set(None);
                    }
                    // Nor will its position: it stays not known (D1362).
                    root.set(IntSize::ZERO);
                    let placeable = scope.measure(content, constraints);
                    scope.place(&placeable, 0, 0);
                    return scope.layout(placeable.width, placeable.height);
                }
                root.set(scope.window_size());
                let (width, height) = (constraints.max_width, constraints.max_height);
                let placeable = scope.measure(content, Constraints::fixed(width, height));
                scope.place(&placeable, 0, 0);
                scope.layout(width, height)
            }
            _ => {
                let consumed = &self.consumed;
                let ancestor = &self.ancestor;
                let left =
                    consumed.get_left(&density, direction) - ancestor.get_left(&density, direction);
                let top = consumed.get_top(&density) - ancestor.get_top(&density);
                let right = consumed.get_right(&density, direction)
                    - ancestor.get_right(&density, direction);
                let bottom = consumed.get_bottom(&density) - ancestor.get_bottom(&density);
                let (horizontal, vertical) = (left + right, top + bottom);
                let placeable = scope.measure(content, constraints.offset(-horizontal, -vertical));
                let width = constraints.constrain_width(placeable.width + horizontal);
                let height = constraints.constrain_height(placeable.height + vertical);
                scope.place(&placeable, left, top);
                scope.layout(width, height)
            }
        }
    }

    fn min_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: IntrinsicMeasurable,
        height: i32,
    ) -> i32 {
        match self.consumer {
            // As Compose's: its content's.
            Consumer::Recalculate(..) => scope.min_intrinsic_width(content, height),
            _ => Measured(self).min_intrinsic_width(scope, content, height),
        }
    }

    fn min_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: IntrinsicMeasurable,
        width: i32,
    ) -> i32 {
        match self.consumer {
            Consumer::Recalculate(..) => scope.min_intrinsic_height(content, width),
            _ => Measured(self).min_intrinsic_height(scope, content, width),
        }
    }

    fn max_intrinsic_width(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: IntrinsicMeasurable,
        height: i32,
    ) -> i32 {
        match self.consumer {
            Consumer::Recalculate(..) => scope.max_intrinsic_width(content, height),
            _ => Measured(self).max_intrinsic_width(scope, content, height),
        }
    }

    fn max_intrinsic_height(
        &mut self,
        scope: &mut IntrinsicMeasureScope<'_>,
        content: IntrinsicMeasurable,
        width: i32,
    ) -> i32 {
        match self.consumer {
            Consumer::Recalculate(..) => scope.max_intrinsic_height(content, width),
            _ => Measured(self).max_intrinsic_height(scope, content, width),
        }
    }
}

/// A consuming node with the default intrinsics (`measure` in an intrinsic measurement), as
/// Compose's paddings and sizes have.
struct Measured<'a>(&'a mut ConsumingNode);

impl LayoutModifierNode for Measured<'_> {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        self.0.measure(scope, content, constraints)
    }
}

impl compose_ui::GlobalPositionAwareModifierNode for ConsumingNode {
    /// `recalculateWindowInsets`' placement: the window's edges' distance from it, written as
    /// the state its descendants read (Compose writes it as it places; compose-rs learns where
    /// a node is after the layout, which lays out again within the frame, D172).
    fn on_globally_positioned(&mut self, coordinates: LayoutCoordinates) -> Option<Action> {
        let Consumer::Recalculate(value, root) = &self.consumer else { return None };
        // Measured without fixed constraints, it passes its ancestors' consumption on: a position
        // written here would be undone by the next measure, a frame every vsync (D1362).
        let root = root.get();
        if root == IntSize::ZERO {
            return None;
        }
        let position = coordinates.position_in_root();
        let size = coordinates.size();
        let (left, top) = (position.x.round() as i32, position.y.round() as i32);
        let right = root.width - (position.x + size.width as f32).round() as i32;
        let bottom = root.height - (position.y + size.height as f32).round() as i32;
        let insets = Some(Insets::new(left, top, right, bottom));
        if value.get() == insets {
            return None;
        }
        let value = value.clone();
        Some(Action::Run(Box::new(move || value.set(insets))))
    }
}

// ---- Elements -----------------------------------------------------------------------------------

/// `window_insets_padding`'s element.
struct PaddingElement(WindowInsets);

impl ModifierNodeElement for PaddingElement {
    type Node = ConsumingNode;
    fn create(&self) -> ConsumingNode {
        ConsumingNode::new(Consumer::Padding(self.0.clone()))
    }
    fn update(&self, node: &mut ConsumingNode) {
        if let Consumer::Padding(insets) = &mut node.consumer
            && *insets != self.0
        {
            *insets = self.0.clone();
            node.post_insets_invalidated();
        }
    }
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
    fn name(&self) -> &'static str {
        "windowInsetsPadding"
    }
    fn inspectable_properties(&self, info: &mut InspectorInfo) {
        info.name = Some("windowInsetsPadding");
        info.properties.push(("insets", self.0.to_string()));
    }
}

/// The shorthand paddings' element: its name and the system's insets it pads by.
struct SystemPadding(&'static str, fn(&Holder) -> WindowInsets);

impl ModifierNodeElement for SystemPadding {
    type Node = ConsumingNode;
    fn create(&self) -> ConsumingNode {
        ConsumingNode::new(Consumer::SystemPadding(self.1, WindowInsets::default()))
    }
    fn update(&self, node: &mut ConsumingNode) {
        if let Consumer::SystemPadding(getter, _) = &mut node.consumer
            && !std::ptr::fn_addr_eq(*getter, self.1)
        {
            *getter = self.1;
            let Some(handle) = node.handle else { return };
            // The host's insets, which only its context reaches: resolved after the pass.
            let getter = self.1;
            handle.side_effect(move || {
                handle.with_node::<ConsumingNode, _>(|node, context| {
                    if let Consumer::SystemPadding(_, insets) = &mut node.consumer {
                        *insets = getter(&Holder(context.window_insets_source()));
                    }
                    node.insets_invalidated(context);
                });
            });
        }
    }
    fn same(&self, other: &Self) -> bool {
        std::ptr::fn_addr_eq(self.1, other.1)
    }
    fn name(&self) -> &'static str {
        self.0
    }
}

/// `consume_window_insets(insets)`'s element.
struct ConsumeElement(WindowInsets);

impl ModifierNodeElement for ConsumeElement {
    type Node = ConsumingNode;
    fn create(&self) -> ConsumingNode {
        ConsumingNode::new(Consumer::Consume(self.0.clone()))
    }
    fn update(&self, node: &mut ConsumingNode) {
        if let Consumer::Consume(insets) = &mut node.consumer
            && *insets != self.0
        {
            *insets = self.0.clone();
            node.post_insets_invalidated();
        }
    }
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
    fn name(&self) -> &'static str {
        "consumeWindowInsets"
    }
    fn inspectable_properties(&self, info: &mut InspectorInfo) {
        info.name = Some("consumeWindowInsets");
        info.properties.push(("insets", self.0.to_string()));
    }
}

/// `consume_window_insets(padding)`'s element.
struct ConsumePaddingElement(PaddingValues);

impl ModifierNodeElement for ConsumePaddingElement {
    type Node = ConsumingNode;
    fn create(&self) -> ConsumingNode {
        ConsumingNode::new(Consumer::ConsumePadding(self.0))
    }
    fn update(&self, node: &mut ConsumingNode) {
        if let Consumer::ConsumePadding(padding) = &mut node.consumer
            && *padding != self.0
        {
            *padding = self.0;
            node.post_insets_invalidated();
        }
    }
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
    fn name(&self) -> &'static str {
        "consumeWindowInsets"
    }
    fn inspectable_properties(&self, info: &mut InspectorInfo) {
        info.name = Some("consumeWindowInsets");
        info.properties.push(("paddingValues", format!("{:?}", self.0)));
    }
}

/// `on_consumed_window_insets_changed`'s element.
struct ConsumedChangedElement(Callback<dyn Fn(WindowInsets)>);

impl ModifierNodeElement for ConsumedChangedElement {
    type Node = ConsumingNode;
    fn create(&self) -> ConsumingNode {
        ConsumingNode::new(Consumer::Changed(self.0.clone()))
    }
    fn update(&self, node: &mut ConsumingNode) {
        if let Consumer::Changed(block) = &mut node.consumer
            && !block.ptr_eq(&self.0)
        {
            *block = self.0.clone();
            node.post_insets_invalidated();
        }
    }
    fn same(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
    fn name(&self) -> &'static str {
        "onConsumedWindowInsetsChanged"
    }
}

/// `recalculate_window_insets`' element.
struct RecalculateElement;

impl ModifierNodeElement for RecalculateElement {
    type Node = ConsumingNode;
    fn create(&self) -> ConsumingNode {
        let value = ValueInsets::new(Some(Insets::ZERO));
        ConsumingNode::new(Consumer::Recalculate(value, Cell::new(IntSize::ZERO)))
    }
    fn update(&self, _node: &mut ConsumingNode) {}
    fn same(&self, _other: &Self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "recalculateWindowInsets"
    }
}

/// The size modifiers' element.
struct SizeElement(WindowInsets, Edge);

impl ModifierNodeElement for SizeElement {
    type Node = ConsumingNode;
    fn create(&self) -> ConsumingNode {
        ConsumingNode::new(Consumer::Size(self.0.clone(), self.1, WindowInsets::default()))
    }
    fn update(&self, node: &mut ConsumingNode) {
        let ancestor = node.ancestor.clone();
        if let Consumer::Size(insets, edge, less) = &mut node.consumer
            && (*insets != self.0 || *edge != self.1)
        {
            *insets = self.0.clone();
            *edge = self.1;
            *less = self.0.clone().exclude(ancestor);
        }
    }
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0 && self.1 == other.1
    }
    fn name(&self) -> &'static str {
        match self.1 {
            Edge::Start => "insetsStartWidth",
            Edge::End => "insetsEndWidth",
            Edge::Top => "insetsTopHeight",
            Edge::Bottom => "insetsBottomHeight",
        }
    }
    fn inspectable_properties(&self, info: &mut InspectorInfo) {
        info.name = Some(self.name());
        info.properties.push(("insets", self.0.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sides(insets: &WindowInsets) -> [i32; 4] {
        let density = Density::new(1.0);
        [
            insets.get_left(&density, LayoutDirection::Ltr),
            insets.get_top(&density),
            insets.get_right(&density, LayoutDirection::Ltr),
            insets.get_bottom(&density),
        ]
    }

    // Compose: compose/foundation/foundation-layout/src/androidHostTest/kotlin/androidx/compose/foundation/layout/WindowInsetsTest.kt, updateConsumeWindowInsetsWhileUnattached,
    // updateConsumeWindowInsetsPaddingValuesWhileUnattached and updateWindowInsetsPaddingWhileUnattached
    #[test]
    fn an_update_while_unattached_changes_what_its_node_consumes() {
        let mut consume = ConsumeElement(WindowInsets::new(1, 2, 3, 4)).create();
        assert!(consume.handle.is_none());
        ConsumeElement(WindowInsets::new(5, 6, 7, 8)).update(&mut consume);
        assert_eq!(sides(&consume.calculate_insets(&WindowInsets::default())), [5, 6, 7, 8]);

        let padding = |values: [f32; 4]| {
            PaddingValues::new(Dp(values[0]), Dp(values[1]), Dp(values[2]), Dp(values[3]))
        };
        let mut consume_padding = ConsumePaddingElement(padding([1.0, 2.0, 3.0, 4.0])).create();
        ConsumePaddingElement(padding([5.0, 6.0, 7.0, 8.0])).update(&mut consume_padding);
        assert_eq!(
            sides(&consume_padding.calculate_insets(&WindowInsets::default())),
            [5, 6, 7, 8]
        );

        let mut pad = PaddingElement(WindowInsets::new(1, 2, 3, 4)).create();
        PaddingElement(WindowInsets::new(5, 6, 7, 8)).update(&mut pad);
        assert_eq!(sides(&pad.calculate_insets(&WindowInsets::default())), [5, 6, 7, 8]);
    }

    // Compose: compose/foundation/foundation-layout/src/androidHostTest/kotlin/androidx/compose/foundation/layout/WindowInsetsTest.kt, updateOnConsumedWindowInsetsChangedWhileUnattached
    #[test]
    fn an_update_while_unattached_changes_the_block_called() {
        let (first, second) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
        let (called_first, called_second) = (first.clone(), second.clone());
        let element = |called: Rc<Cell<bool>>| {
            ConsumedChangedElement(IntoCallback::<dyn Fn(WindowInsets)>::into_callback(
                move |_: WindowInsets| called.set(true),
            ))
        };
        let mut node = element(called_first).create();
        element(called_second).update(&mut node);
        node.calculate_insets(&WindowInsets::default());
        assert!(!first.get());
        assert!(second.get());
    }
}
