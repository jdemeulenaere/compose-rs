//! What a gesture says, as AndroidX's `NavigationEvent`, `NavigationEventTransitionState`,
//! `NavigationEventInfo` and `NavigationEventHistory`.

use std::any::Any;
use std::fmt;
use std::rc::Rc;

/// A moment of a gesture, as AndroidX's `NavigationEvent`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct NavigationEvent {
    /// Where the gesture started: [`EDGE_LEFT`](Self::EDGE_LEFT), [`EDGE_RIGHT`](Self::EDGE_RIGHT)
    /// or [`EDGE_NONE`](Self::EDGE_NONE).
    pub swipe_edge: i32,
    /// How far it's gone, from 0 to 1.
    pub progress: f32,
    pub touch_x: f32,
    pub touch_y: f32,
    pub frame_time_millis: i64,
}

impl NavigationEvent {
    pub const EDGE_LEFT: i32 = 0;
    pub const EDGE_RIGHT: i32 = 1;
    pub const EDGE_NONE: i32 = 2;
}

impl Default for NavigationEvent {
    fn default() -> NavigationEvent {
        NavigationEvent {
            swipe_edge: NavigationEvent::EDGE_NONE,
            progress: 0.0,
            touch_x: 0.0,
            touch_y: 0.0,
            frame_time_millis: 0,
        }
    }
}

/// Whether a gesture is under way, as AndroidX's `NavigationEventTransitionState`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum NavigationEventTransitionState {
    Idle,
    InProgress {
        latest_event: NavigationEvent,
        /// [`TRANSITIONING_BACK`](Self::TRANSITIONING_BACK) or
        /// [`TRANSITIONING_FORWARD`](Self::TRANSITIONING_FORWARD).
        direction: i32,
    },
}

impl NavigationEventTransitionState {
    pub(crate) const TRANSITIONING_UNKNOWN: i32 = 0;
    pub const TRANSITIONING_FORWARD: i32 = 1;
    pub const TRANSITIONING_BACK: i32 = -1;
}

trait ErasedInfo {
    fn as_any(&self) -> &dyn Any;
    fn same(&self, other: &dyn ErasedInfo) -> bool;
    fn debug(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result;
}

impl<T: PartialEq + fmt::Debug + 'static> ErasedInfo for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn same(&self, other: &dyn ErasedInfo) -> bool {
        other.as_any().downcast_ref::<T>().is_some_and(|other| self == other)
    }

    fn debug(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

/// What a place in the navigation is, as AndroidX's `NavigationEventInfo`: any value, compared with
/// `==`, read back with [`downcast_ref`](Self::downcast_ref), where AndroidX's is a class to extend.
#[derive(Clone)]
pub struct NavigationEventInfo {
    value: Rc<dyn ErasedInfo>,
    title: Option<String>,
    url: Option<String>,
}

impl NavigationEventInfo {
    pub fn new(value: impl PartialEq + fmt::Debug + 'static) -> NavigationEventInfo {
        NavigationEventInfo { value: Rc::new(value), title: None, url: None }
    }

    /// Nothing in particular, as `NavigationEventInfo.None`.
    pub fn none() -> NavigationEventInfo {
        #[derive(PartialEq, Debug)]
        struct None;
        NavigationEventInfo::new(None)
    }

    /// With a title, as an overridden `title`.
    pub fn with_title(self, title: impl Into<String>) -> NavigationEventInfo {
        NavigationEventInfo { title: Some(title.into()), ..self }
    }

    /// With a URL, as an overridden `url`.
    pub fn with_url(self, url: impl Into<String>) -> NavigationEventInfo {
        NavigationEventInfo { url: Some(url.into()), ..self }
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    /// The value it was made of, if it's a `T`.
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.value.as_any().downcast_ref()
    }
}

impl PartialEq for NavigationEventInfo {
    fn eq(&self, other: &NavigationEventInfo) -> bool {
        self.value.same(&*other.value) && self.title == other.title && self.url == other.url
    }
}

impl fmt::Debug for NavigationEventInfo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.debug(formatter)
    }
}

/// The places a dispatcher's enabled handlers tell of, as AndroidX's `NavigationEventHistory`.
#[derive(Clone, PartialEq)]
pub struct NavigationEventHistory {
    merged_history: Vec<NavigationEventInfo>,
    current_index: i32,
}

impl Default for NavigationEventHistory {
    /// Nothing, at -1.
    fn default() -> NavigationEventHistory {
        NavigationEventHistory { merged_history: Vec::new(), current_index: -1 }
    }
}

impl NavigationEventHistory {
    /// `back_info`, `current_info`, `forward_info`, at `current_info`.
    pub fn new(
        current_info: NavigationEventInfo,
        back_info: Vec<NavigationEventInfo>,
        forward_info: Vec<NavigationEventInfo>,
    ) -> NavigationEventHistory {
        let current_index = back_info.len() as i32;
        let mut merged_history = back_info;
        merged_history.push(current_info);
        merged_history.extend(forward_info);
        NavigationEventHistory { merged_history, current_index }
    }

    pub fn merged_history(&self) -> &[NavigationEventInfo] {
        &self.merged_history
    }

    pub fn current_index(&self) -> i32 {
        self.current_index
    }
}

impl fmt::Debug for NavigationEventHistory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "NavigationEventHistory(currentIndex={}, mergedHistory={:?})",
            self.current_index, self.merged_history
        )
    }
}
