//! Compose's `foundation.gestures`: `Modifier.draggable` (drags along one axis reported as
//! deltas to a `DraggableState`), and scrolling (`scrollable`: `ScrollableState`,
//! `Modifier.scrollable`, flings).

mod drag_gesture;
mod draggable;
mod mouse_wheel;
mod scrollable;
mod snapping;

pub use draggable::{
    DefaultDraggable2DState, DefaultDraggableState, Drag2DBlock, Drag2DScope, DragBlock, DragScope,
    Draggable2DOptions, Draggable2DState, Draggable2DStateExt, DraggableModifier, DraggableOptions,
    DraggableState, DraggableStateExt, OnDrag2DStopped, OnDragStarted, OnDragStopped,
    draggable_2d_state, draggable_state, remember_draggable_2d_state, remember_draggable_state,
};

pub(crate) use drag_gesture::{DRAG_GESTURES, GestureState, aligned, parent_aligned};
pub(crate) use mouse_wheel::MouseWheel;
pub use scrollable::{
    DefaultFlingBehavior, DefaultScrollableState, FlingBehavior, LocalBoxFuture, ScrollBlock,
    ScrollScope, ScrollableDefaults, ScrollableModifier, ScrollableState, ScrollableStateExt,
    remember_scrollable_state, scrollable_state,
};
pub(crate) use scrollable::{DragSession, ScrollCore};
pub use snapping::{
    MIN_FLING_VELOCITY, SnapFlingBehavior, SnapLayoutInfoProvider, SnapPosition,
    TargetedFlingBehavior, grid_snap_layout_info_provider, list_snap_layout_info_provider,
    remember_grid_snap_fling_behavior, remember_list_snap_fling_behavior,
    remember_snap_fling_behavior, snap_fling_behavior,
};

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
