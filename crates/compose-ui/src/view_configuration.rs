//! The platform's gesture timeouts and distances, as Compose's `ViewConfiguration` and
//! `LocalViewConfiguration`: what the gesture detectors wait for and how far a pointer moves
//! before it drags (D907).

use compose_runtime::{ComposeArg, CompositionLocal};

use crate::unit::{Density, Dp, DpSize};

/// Gesture timeouts and distances, as Compose's `ViewConfiguration`. The defaults are
/// Android's (Compose's `AndroidViewConfiguration` reads them from the platform):
/// [`ViewConfiguration::at`].
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ViewConfiguration {
    /// How long a press lasts before it's a long press, as `longPressTimeoutMillis`.
    pub long_press_timeout_millis: u64,
    /// How long after a tap a second one makes a double tap, as `doubleTapTimeoutMillis`.
    pub double_tap_timeout_millis: u64,
    /// A second tap sooner than this isn't a double tap, as `doubleTapMinTimeMillis`.
    pub double_tap_min_time_millis: u64,
    /// How far, in pixels, a pointer moves before it's a drag, as `touchSlop`.
    pub touch_slop: f32,
    /// How far, in pixels, a stylus moves before it's handwriting, as `handwritingSlop`.
    pub handwriting_slop: f32,
    /// The smallest size of a touch target, as `minimumTouchTargetSize`.
    pub minimum_touch_target_size: DpSize,
    /// The fastest fling, in pixels a second, as `maximumFlingVelocity`.
    pub maximum_fling_velocity: f32,
    /// The slowest fling, in pixels a second, as `minimumFlingVelocity`.
    pub minimum_fling_velocity: f32,
    /// The margin, in pixels, around a text line where stylus handwriting gestures apply, as
    /// `handwritingGestureLineMargin`.
    pub handwriting_gesture_line_margin: f32,
}

impl ViewConfiguration {
    /// How far a pointer of `pointer_type` moves before it's a drag, as Compose's `pointerSlop`: a
    /// mouse's 0.125 dp against a finger's 18 dp, for it has no tremor to tell from a drag.
    pub fn pointer_slop(&self, pointer_type: crate::PointerType) -> f32 {
        match pointer_type {
            crate::PointerType::Mouse => self.touch_slop * (0.125 / 18.0),
            _ => self.touch_slop,
        }
    }

    /// Android's defaults at `density`: a long press at 400 ms, double taps 40 to 300 ms
    /// apart, an 8 dp touch slop, a 2 dp handwriting slop, 48 dp touch targets, flings between
    /// 50 and 8,000 dp a second, a 16 dp handwriting gesture line margin (Android's
    /// `getScaledHandwritingSlop` and `getScaledHandwritingGestureLineMargin`).
    pub fn at(density: Density) -> ViewConfiguration {
        ViewConfiguration {
            long_press_timeout_millis: 400,
            double_tap_timeout_millis: 300,
            double_tap_min_time_millis: 40,
            touch_slop: density.to_px(Dp(8.0)),
            handwriting_slop: density.to_px(Dp(2.0)),
            minimum_touch_target_size: DpSize::new(Dp(48.0), Dp(48.0)),
            maximum_fling_velocity: density.to_px(Dp(8000.0)),
            minimum_fling_velocity: density.to_px(Dp(50.0)),
            handwriting_gesture_line_margin: density.to_px(Dp(16.0)),
        }
    }
}

impl Default for ViewConfiguration {
    /// [`ViewConfiguration::at`] density 1.
    fn default() -> ViewConfiguration {
        ViewConfiguration::at(Density::default())
    }
}

impl ComposeArg for ViewConfiguration {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// The view configuration where it's read, as `LocalViewConfiguration`: what the gesture
/// detectors use, each layout node's being the one where it's composed. The host provides its
/// own (`UiHost::set_view_configuration`); provide another for a subtree, as Compose's tests do.
pub static LOCAL_VIEW_CONFIGURATION: CompositionLocal<ViewConfiguration> =
    CompositionLocal::new_static("LocalViewConfiguration", ViewConfiguration::default);

impl crate::tree::UiTree {
    /// The view configuration the gesture detectors use: the host's
    /// (`UiHost::set_view_configuration`), else Android's at the tree's density.
    pub fn view_configuration(&self) -> ViewConfiguration {
        self.view_configuration.unwrap_or_else(|| ViewConfiguration::at(self.density))
    }

    /// The view configuration where `node` is composed (the host's, for what isn't composed).
    /// The view configuration of `node`, as `LayoutInfo.viewConfiguration`.
    pub fn view_configuration_of(&self, node: crate::tree::NodeId) -> ViewConfiguration {
        match self.get(node).map_or(0, |value| value.view_configuration) {
            0 => self.view_configuration(),
            index => self.view_configurations[index as usize - 1],
        }
    }

    /// Sets the view configuration (`None`: Android's at the density).
    pub(crate) fn set_view_configuration(&mut self, configuration: Option<ViewConfiguration>) {
        self.view_configuration = configuration;
    }
}
