//! Overriding device-wide configuration for one piece of content, as Compose's
//! `DeviceConfigurationOverride`: the layout direction and dark mode compose-rs's locals carry.

use std::rc::Rc;

use compose_runtime::{Callback, ComposeArg, CompositionLocalProvider, composable};
use compose_ui::{LOCAL_CONFIGURATION, LOCAL_DENSITY, LOCAL_LAYOUT_DIRECTION, LayoutDirection};

/// An override applied to the content it wraps, as Compose's `DeviceConfigurationOverride`
/// fun interface: its `apply` composes the content once, under the overridden values.
#[derive(Clone)]
pub struct DeviceConfigurationOverride {
    apply: Rc<dyn Fn(&Callback)>,
}

impl ComposeArg for DeviceConfigurationOverride {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.apply, &other.apply)
    }
}

impl DeviceConfigurationOverride {
    /// An override of its own: `apply` composes the content under test exactly once, as
    /// Compose's `Override(contentUnderTest)`.
    pub fn new(apply: impl Fn(&Callback) + 'static) -> DeviceConfigurationOverride {
        DeviceConfigurationOverride { apply: Rc::new(apply) }
    }

    /// The content laid out in `direction`, as `DeviceConfigurationOverride.LayoutDirection`.
    pub fn layout_direction(direction: LayoutDirection) -> DeviceConfigurationOverride {
        DeviceConfigurationOverride::new(move |content| {
            CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), &**content);
        })
    }

    /// The content in dark mode or not (`LOCAL_CONFIGURATION`'s `night_mode`, which
    /// `is_system_in_dark_theme` reads), as `DeviceConfigurationOverride.DarkMode`.
    pub fn dark_mode(is_dark_mode: bool) -> DeviceConfigurationOverride {
        DeviceConfigurationOverride::new(move |content| {
            let mut configuration = LOCAL_CONFIGURATION.current();
            configuration.night_mode = is_dark_mode;
            CompositionLocalProvider(LOCAL_CONFIGURATION.provides(configuration), &**content);
        })
    }

    /// The content at `font_scale`, its density otherwise as it was, as
    /// `DeviceConfigurationOverride.FontScale`. A node takes its density where it's composed
    /// (D1183), so its text measures at it.
    pub fn font_scale(font_scale: f32) -> DeviceConfigurationOverride {
        DeviceConfigurationOverride::new(move |content| {
            let mut density = LOCAL_DENSITY.current();
            density.font_scale = font_scale;
            let mut configuration = LOCAL_CONFIGURATION.current();
            configuration.font_scale = font_scale;
            CompositionLocalProvider(
                (LOCAL_DENSITY.provides(density), LOCAL_CONFIGURATION.provides(configuration)),
                &**content,
            );
        })
    }

    /// This override around `other`'s, as Compose's `then`: where both set a value, `other`'s
    /// (the inner one) wins.
    pub fn then(self, other: DeviceConfigurationOverride) -> DeviceConfigurationOverride {
        DeviceConfigurationOverride::new(move |content| {
            let other = other.clone();
            let content = content.clone();
            let inner: Callback = compose_runtime::callback(move || (other.apply)(&content));
            (self.apply)(&inner);
        })
    }
}

/// `content` under `configuration`, as Compose's `DeviceConfigurationOverride(override) { … }`.
#[composable]
#[allow(non_snake_case)]
pub fn DeviceConfigurationOverride(
    configuration: DeviceConfigurationOverride,
    content: impl Fn() + 'static,
) {
    let content: Callback = compose_runtime::callback(content);
    (configuration.apply)(&content);
}
