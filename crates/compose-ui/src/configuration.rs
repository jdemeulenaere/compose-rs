//! What the platform tells the content, as Compose's `androidx.compose.ui.platform` locals: the
//! configuration ([`LOCAL_CONFIGURATION`]: screen size, orientation, font scale, density, dark
//! mode), the window's insets ([`LOCAL_PLATFORM_WINDOW_INSETS`]: system bars, the keyboard,
//! a display cutout), and, for an app embedded in a platform UI, the properties the platform
//! passes it and the events it sends back ([`LOCAL_HOST_PROPS`], [`LOCAL_HOST_EVENTS`]). The
//! host provides them (`UiHost::set_configuration`, `set_window_insets`, `props`,
//! `set_event_listener`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use compose_runtime::{CompositionLocal, MutableState, OwnedState, mutable_state_of};

use crate::unit::{Density, IntSize};

/// The screen's orientation, as Android's `Configuration.orientation`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ScreenOrientation {
    #[default]
    Portrait,
    Landscape,
}

/// The platform's configuration, as the parts of Android's `Configuration` Compose apps read
/// through `LocalConfiguration`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Configuration {
    /// The screen's (on desktop, the window's) width in dp, as `screenWidthDp`.
    pub screen_width_dp: f32,
    pub screen_height_dp: f32,
    pub orientation: ScreenOrientation,
    /// The user's text size preference, as `fontScale`.
    pub font_scale: f32,
    /// Pixels per dp, as `densityDpi / 160`.
    pub density: f32,
    /// The system's dark theme is on, as `uiMode`'s `UI_MODE_NIGHT_YES`.
    pub night_mode: bool,
}

impl Default for Configuration {
    fn default() -> Configuration {
        Configuration::from_window(IntSize::new(0, 0), Density::default(), false)
    }
}

impl Configuration {
    /// The configuration of a window of `size` pixels at `density`: what desktop hosts provide.
    pub fn from_window(size: IntSize, density: Density, night_mode: bool) -> Configuration {
        let scale = density.density.max(f32::MIN_POSITIVE);
        let (width, height) = (size.width as f32 / scale, size.height as f32 / scale);
        Configuration {
            screen_width_dp: width,
            screen_height_dp: height,
            orientation: if width > height {
                ScreenOrientation::Landscape
            } else {
                ScreenOrientation::Portrait
            },
            font_scale: density.font_scale,
            density: density.density,
            night_mode,
        }
    }
}

impl compose_runtime::ComposeArg for Configuration {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// The platform's configuration, as Compose's `LocalConfiguration`.
pub static LOCAL_CONFIGURATION: CompositionLocal<Configuration> =
    CompositionLocal::new("LocalConfiguration", Configuration::default);

/// Space at each edge of the window, in pixels.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Insets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Insets {
    pub const ZERO: Insets = Insets { left: 0, top: 0, right: 0, bottom: 0 };

    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Insets {
        Insets { left, top, right, bottom }
    }

    /// The larger of the two at each edge, as `WindowInsets.union`.
    pub fn union(self, other: Insets) -> Insets {
        Insets {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }
}

/// A type of the window's insets, as Android's `WindowInsetsCompat.Type` (and the waterfall
/// insets of a display cutout): what [`PlatformWindowInsets`] holds one of each of.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum InsetsType {
    StatusBars,
    NavigationBars,
    CaptionBar,
    /// The soft keyboard.
    Ime,
    DisplayCutout,
    /// The status, navigation and caption bars: Android's `systemBars()`, which the platform
    /// reports apart (its insets for the three types at once).
    SystemBars,
    SystemGestures,
    MandatorySystemGestures,
    TappableElement,
    /// A waterfall display's curved edges (`DisplayCutoutCompat.waterfallInsets`).
    Waterfall,
}

impl InsetsType {
    /// The types, in [`PlatformWindowInsets`]' order.
    pub const ALL: [InsetsType; 10] = [
        InsetsType::StatusBars,
        InsetsType::NavigationBars,
        InsetsType::CaptionBar,
        InsetsType::Ime,
        InsetsType::DisplayCutout,
        InsetsType::SystemBars,
        InsetsType::SystemGestures,
        InsetsType::MandatorySystemGestures,
        InsetsType::TappableElement,
        InsetsType::Waterfall,
    ];

    const fn bit(self) -> u16 {
        1 << self as u16
    }
}

/// The window's insets as the platform reports them (Android's `WindowInsetsCompat` by type,
/// from `setOnApplyWindowInsetsListener` and, while they animate, `WindowInsetsAnimation`'s
/// progress): what the system draws over the content, whether each type is visible, and the
/// insets each would take shown. All zero, and visible, on desktop. What foundation's
/// `WindowInsets` read (Compose's `WindowInsetsHolder`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PlatformWindowInsets {
    pub status_bars: Insets,
    pub navigation_bars: Insets,
    pub caption_bar: Insets,
    /// The soft keyboard.
    pub ime: Insets,
    pub display_cutout: Insets,
    /// The status, navigation and caption bars, as the platform reports them together
    /// (`getInsets(systemBars())`).
    pub system_bars: Insets,
    pub system_gestures: Insets,
    pub mandatory_system_gestures: Insets,
    pub tappable_element: Insets,
    pub waterfall: Insets,
    /// The insets of the status bars were they shown (`getInsetsIgnoringVisibility`).
    pub status_bars_ignoring_visibility: Insets,
    pub navigation_bars_ignoring_visibility: Insets,
    pub caption_bar_ignoring_visibility: Insets,
    pub system_bars_ignoring_visibility: Insets,
    pub tappable_element_ignoring_visibility: Insets,
    /// The keyboard's insets where its animation started, and where it will end, as Compose's
    /// `imeAnimationSource` and `imeAnimationTarget` (the keyboard's insets when none runs).
    pub ime_animation_source: Insets,
    pub ime_animation_target: Insets,
    /// Which types are shown: see [`is_visible`](Self::is_visible).
    pub visibility: InsetsVisibility,
}

/// Which types of insets are shown, as `WindowInsetsCompat.isVisible(type)`: all, by default.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct InsetsVisibility {
    /// The types hidden, by bit.
    hidden: u16,
}

impl InsetsVisibility {
    /// Whether `kind` is shown.
    pub fn is_visible(&self, kind: InsetsType) -> bool {
        self.hidden & kind.bit() == 0
    }

    /// Shows or hides `kind`.
    pub fn set(&mut self, kind: InsetsType, visible: bool) {
        if visible {
            self.hidden &= !kind.bit();
        } else {
            self.hidden |= kind.bit();
        }
    }
}

impl Default for PlatformWindowInsets {
    /// None, every type visible (Compose's default before the platform reports any).
    fn default() -> PlatformWindowInsets {
        PlatformWindowInsets {
            status_bars: Insets::ZERO,
            navigation_bars: Insets::ZERO,
            caption_bar: Insets::ZERO,
            ime: Insets::ZERO,
            display_cutout: Insets::ZERO,
            system_bars: Insets::ZERO,
            system_gestures: Insets::ZERO,
            mandatory_system_gestures: Insets::ZERO,
            tappable_element: Insets::ZERO,
            waterfall: Insets::ZERO,
            status_bars_ignoring_visibility: Insets::ZERO,
            navigation_bars_ignoring_visibility: Insets::ZERO,
            caption_bar_ignoring_visibility: Insets::ZERO,
            system_bars_ignoring_visibility: Insets::ZERO,
            tappable_element_ignoring_visibility: Insets::ZERO,
            ime_animation_source: Insets::ZERO,
            ime_animation_target: Insets::ZERO,
            visibility: InsetsVisibility::default(),
        }
    }
}

impl PlatformWindowInsets {
    /// The insets of `kind`.
    pub fn get(&self, kind: InsetsType) -> Insets {
        match kind {
            InsetsType::StatusBars => self.status_bars,
            InsetsType::NavigationBars => self.navigation_bars,
            InsetsType::CaptionBar => self.caption_bar,
            InsetsType::Ime => self.ime,
            InsetsType::DisplayCutout => self.display_cutout,
            InsetsType::SystemBars => self.system_bars,
            InsetsType::SystemGestures => self.system_gestures,
            InsetsType::MandatorySystemGestures => self.mandatory_system_gestures,
            InsetsType::TappableElement => self.tappable_element,
            InsetsType::Waterfall => self.waterfall,
        }
    }

    /// Sets the insets of `kind`.
    pub fn set(&mut self, kind: InsetsType, insets: Insets) {
        *match kind {
            InsetsType::StatusBars => &mut self.status_bars,
            InsetsType::NavigationBars => &mut self.navigation_bars,
            InsetsType::CaptionBar => &mut self.caption_bar,
            InsetsType::Ime => &mut self.ime,
            InsetsType::DisplayCutout => &mut self.display_cutout,
            InsetsType::SystemBars => &mut self.system_bars,
            InsetsType::SystemGestures => &mut self.system_gestures,
            InsetsType::MandatorySystemGestures => &mut self.mandatory_system_gestures,
            InsetsType::TappableElement => &mut self.tappable_element,
            InsetsType::Waterfall => &mut self.waterfall,
        } = insets;
    }

    /// Whether `kind` is shown, as `WindowInsetsCompat.isVisible(type)`: true until the
    /// platform says otherwise.
    pub fn is_visible(&self, kind: InsetsType) -> bool {
        self.visibility.is_visible(kind)
    }

    /// Sets whether `kind` is shown.
    pub fn set_visible(&mut self, kind: InsetsType, visible: bool) {
        self.visibility.set(kind, visible);
    }

    /// These insets with `kind`'s set: for building them in tests.
    pub fn with(mut self, kind: InsetsType, insets: Insets) -> PlatformWindowInsets {
        self.set(kind, insets);
        self
    }
}

impl compose_runtime::ComposeArg for PlatformWindowInsets {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// The window's insets, for content reading them in composition (a change recomposes it);
/// foundation's `WindowInsets` read them through [`LOCAL_WINDOW_INSETS_SOURCE`] where they're
/// used instead.
pub static LOCAL_PLATFORM_WINDOW_INSETS: CompositionLocal<PlatformWindowInsets> =
    CompositionLocal::new("LocalPlatformWindowInsets", PlatformWindowInsets::default);

/// The host's window insets, as the state it sets them in (`UiHost::set_window_insets`): what
/// foundation's system `WindowInsets` (`status_bars()`, `ime()`, …) keep and read where they're
/// used, so a read while measuring is observed and only the layouts that read it measure again
/// (Compose's `WindowInsetsHolder`, one per view). Without a host, none: zero insets.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct WindowInsetsSource(
    Option<(MutableState<PlatformWindowInsets>, MutableState<CutoutPath>)>,
);

impl WindowInsetsSource {
    /// No host: zero insets, every type visible.
    pub const NONE: WindowInsetsSource = WindowInsetsSource(None);

    pub(crate) fn of(
        insets: MutableState<PlatformWindowInsets>,
        cutout_path: MutableState<CutoutPath>,
    ) -> WindowInsetsSource {
        WindowInsetsSource(Some((insets, cutout_path)))
    }

    /// The insets now: a read of the host's state (observed while composing, measuring or
    /// drawing). Zero once the host is gone.
    pub fn get(&self) -> PlatformWindowInsets {
        self.0.and_then(|(insets, _)| insets.try_get()).unwrap_or_default()
    }

    /// The display cutout's outline now, if the platform reports one: a read of the host's state,
    /// as Compose's `WindowInsetsHolder.cutoutPath`.
    pub fn cutout_path(&self) -> Option<crate::path::Path> {
        self.0.and_then(|(_, path)| path.try_get()).and_then(|path| path.0)
    }
}

/// A display cutout's outline, as state: a new path is a change, as a `Path` in Compose's
/// `mutableStateOf` (it has no structural equality).
#[derive(Clone, Default)]
pub(crate) struct CutoutPath(pub(crate) Option<crate::path::Path>);

impl PartialEq for CutoutPath {
    fn eq(&self, other: &CutoutPath) -> bool {
        match (&self.0, &other.0) {
            (Some(first), Some(second)) => first.ptr_eq(second),
            (None, None) => true,
            _ => false,
        }
    }
}

impl compose_runtime::ComposeArg for WindowInsetsSource {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// The host's [`WindowInsetsSource`], provided around its content (as Compose's `LocalView`
/// leads to its `WindowInsetsHolder`).
pub static LOCAL_WINDOW_INSETS_SOURCE: CompositionLocal<WindowInsetsSource> =
    CompositionLocal::new("LocalWindowInsetsSource", WindowInsetsSource::default);

impl crate::node_context::NodeContext<'_> {
    /// The window insets of the node's host, as Compose's nodes reach their view's
    /// `WindowInsetsHolder` (`requireView()`): what an insets padding attached outside
    /// composition reads (`Modifier.system_bars_padding()`).
    pub fn window_insets_source(&self) -> WindowInsetsSource {
        self.tree.window_insets
    }
}

/// A property the platform passes to the content.
#[derive(Clone, PartialEq, Debug)]
pub enum PropValue {
    Text(Rc<str>),
    Number(f64),
    Bool(bool),
}

/// The properties the platform passes to an embedded app (Kotlin's `ComposeRsView.setProp`),
/// by name: reading one in composition recomposes the reader when the platform changes it.
#[derive(Clone, Default)]
pub struct HostProps {
    values: Rc<RefCell<HashMap<String, OwnedState<Option<PropValue>>>>>,
}

impl compose_runtime::ComposeArg for HostProps {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.values, &other.values)
    }
}

impl HostProps {
    fn state(&self, name: &str) -> MutableState<Option<PropValue>> {
        if let Some(state) = self.values.borrow().get(name) {
            return state.handle();
        }
        let state = mutable_state_of(None);
        let handle = state.handle();
        self.values.borrow_mut().insert(name.to_owned(), state);
        handle
    }

    /// The property `name`, if the platform set it. Read in composition, a change recomposes
    /// the reader.
    pub fn get(&self, name: &str) -> Option<PropValue> {
        self.state(name).get()
    }

    pub fn string(&self, name: &str) -> Option<Rc<str>> {
        match self.get(name)? {
            PropValue::Text(value) => Some(value),
            _ => None,
        }
    }

    pub fn number(&self, name: &str) -> Option<f64> {
        match self.get(name)? {
            PropValue::Number(value) => Some(value),
            _ => None,
        }
    }

    pub fn boolean(&self, name: &str) -> Option<bool> {
        match self.get(name)? {
            PropValue::Bool(value) => Some(value),
            _ => None,
        }
    }

    /// Sets (or with `None` removes) the property `name`: the platform's side.
    pub fn set(&self, name: &str, value: Option<PropValue>) {
        self.state(name).set(value);
    }
}

/// The properties the platform passes ([`HostProps`]): empty outside an embedding platform.
pub static LOCAL_HOST_PROPS: CompositionLocal<HostProps> =
    CompositionLocal::new("LocalHostProps", HostProps::default);

type EventListener = Rc<dyn Fn(&str, &str)>;

/// Where an embedded app sends events to the platform (Kotlin's
/// `ComposeRsView.setOnEventListener`): a name and a payload.
#[derive(Clone, Default)]
pub struct HostEvents {
    listener: Rc<RefCell<Option<EventListener>>>,
}

impl compose_runtime::ComposeArg for HostEvents {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.listener, &other.listener)
    }
}

impl HostEvents {
    /// Sends the event `name` with `payload` to the platform's listener, if it has one. Returns
    /// whether it had.
    pub fn send(&self, name: &str, payload: &str) -> bool {
        let listener = self.listener.borrow().clone();
        match listener {
            Some(listener) => {
                listener(name, payload);
                true
            }
            None => false,
        }
    }

    /// Sets the platform's listener: the host's side.
    pub fn set_listener(&self, listener: Option<EventListener>) {
        *self.listener.borrow_mut() = listener;
    }
}

/// Where the content sends events to the platform ([`HostEvents`]).
pub static LOCAL_HOST_EVENTS: CompositionLocal<HostEvents> =
    CompositionLocal::new("LocalHostEvents", HostEvents::default);
