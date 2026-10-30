//! Compose's `foundation.gestures`: `Modifier.draggable` (drags along one axis reported as
//! deltas to a `DraggableState`), and scrolling (`scrollable`: `ScrollableState`,
//! `Modifier.scrollable`, flings).

mod drag_gesture;
mod draggable;
mod mouse_wheel;
mod scrollable;

pub use draggable::{
    Drag2DScope, DragScope, Draggable2DOptions, Draggable2DState, DraggableModifier,
    DraggableOptions, DraggableState, OnDrag2DStopped, OnDragStarted, OnDragStopped,
    remember_draggable_2d_state, remember_draggable_state,
};

pub(crate) use mouse_wheel::MouseWheel;
pub use scrollable::{
    DefaultFlingBehavior, DefaultScrollableState, FlingBehavior, LocalBoxFuture, ScrollBlock,
    ScrollScope, ScrollableDefaults, ScrollableModifier, ScrollableState, ScrollableStateExt,
    remember_scrollable_state, scrollable_state,
};
pub(crate) use scrollable::{DragSession, ScrollCore};

use compose_runtime::ComposeArg;

/// An axis, as Compose's `Orientation`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Orientation {
    Vertical,
    Horizontal,
}

impl ComposeArg for Orientation {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}
