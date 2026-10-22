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

/// The window's insets as the platform reports them (Android's `WindowInsets` by type, from
/// `setOnApplyWindowInsetsListener`): what the system draws over the content. Zero on desktop.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PlatformWindowInsets {
    pub status_bars: Insets,
    pub navigation_bars: Insets,
    pub caption_bar: Insets,
    /// The soft keyboard.
    pub ime: Insets,
    pub display_cutout: Insets,
}

impl compose_runtime::ComposeArg for PlatformWindowInsets {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// The window's insets, for content reading them in composition (foundation's
/// `WindowInsets::as_padding_values`); the insets paddings read the host's while measuring.
pub static LOCAL_PLATFORM_WINDOW_INSETS: CompositionLocal<PlatformWindowInsets> =
    CompositionLocal::new("LocalPlatformWindowInsets", PlatformWindowInsets::default);

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
