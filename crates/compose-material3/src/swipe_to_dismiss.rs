//! `SwipeToDismissBox`, as Compose's Material 3: content swiped away sideways, uncovering a
//! background, with its `SwipeToDismissBoxState`.

use std::rc::Rc;

use compose_animation::AnimationSpec;
use compose_foundation::gestures::{DraggableModifier, DraggableState, Orientation};
use compose_foundation::layout::{Arrangement, Row, RowScope};
use compose_runtime::{ComposeArg, composable, remember, remember_coroutine_scope};
use compose_ui::{
    Alignment, Constraints, Dp, Layout, LayoutDirection, Measurable, MeasurePolicy, MeasureResult,
    MeasureScope, Modifier,
};

use crate::anchored::Anchored;

/// Where a swipe-to-dismiss box rests, as Compose's `SwipeToDismissBoxValue`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SwipeToDismissBoxValue {
    /// Swiped away towards the end.
    StartToEnd,
    /// Swiped away towards the start.
    EndToStart,
    /// In place.
    Settled,
}

impl ComposeArg for SwipeToDismissBoxValue {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

struct SwipeInner {
    anchored: Anchored<SwipeToDismissBoxValue>,
    positional_threshold: Box<dyn Fn(f32) -> f32>,
}

/// A swipe-to-dismiss box's state, as Compose's `SwipeToDismissBoxState`.
#[derive(Clone)]
pub struct SwipeToDismissBoxState {
    inner: Rc<SwipeInner>,
}

impl SwipeToDismissBoxState {
    /// As `SwipeToDismissBoxState(initialValue, density, confirmValueChange,
    /// positionalThreshold)`: `confirm_value_change` can refuse a dismissal (the content comes
    /// back); `positional_threshold` gives, from the width, how far (pixels) a swipe must go.
    pub fn new(
        initial_value: SwipeToDismissBoxValue,
        confirm_value_change: impl Fn(SwipeToDismissBoxValue) -> bool + 'static,
        positional_threshold: impl Fn(f32) -> f32 + 'static,
    ) -> SwipeToDismissBoxState {
        SwipeToDismissBoxState {
            inner: Rc::new(SwipeInner {
                anchored: Anchored::new(
                    initial_value,
                    Box::new(confirm_value_change),
                    AnimationSpec::Spring(compose_animation::spring()),
                ),
                positional_threshold: Box::new(positional_threshold),
            }),
        }
    }

    /// Where it last settled (a state read), as `currentValue`.
    pub fn current_value(&self) -> SwipeToDismissBoxValue {
        self.inner.anchored.current_value()
    }

    /// Where it's going, or would settle if let go now (a state read), as `targetValue`.
    pub fn target_value(&self) -> SwipeToDismissBoxValue {
        self.inner.anchored.target_value()
    }

    /// The content's offset, in pixels (a state read), as `requireOffset()`.
    pub fn require_offset(&self) -> f32 {
        self.inner.anchored.offset()
    }

    /// Which way it's being swiped, from the (logical) offset's sign, as `dismissDirection`.
    pub fn dismiss_direction(&self) -> SwipeToDismissBoxValue {
        let offset = self.require_offset();
        if offset == 0.0 || offset.is_nan() {
            SwipeToDismissBoxValue::Settled
        } else if offset > 0.0 {
            SwipeToDismissBoxValue::StartToEnd
        } else {
            SwipeToDismissBoxValue::EndToStart
        }
    }

    /// How far it's been swiped towards a dismissal, 0 to 1, as `progress`.
    pub fn progress(&self) -> f32 {
        let anchored = &self.inner.anchored;
        let offset = self.require_offset();
        let from = anchored.position_of(anchored.current_value()).unwrap_or(0.0);
        let to = anchored.position_of(anchored.target_value()).unwrap_or(from);
        if offset.is_nan() || from == to {
            1.0
        } else {
            ((offset - from) / (to - from)).clamp(0.0, 1.0)
        }
    }

    /// Animates it back in place, as `reset()`.
    pub async fn reset(&self) {
        self.inner.anchored.animate_to(SwipeToDismissBoxValue::Settled).await;
    }

    /// Animates it away in `direction`, as `dismiss(direction)`.
    pub async fn dismiss(&self, direction: SwipeToDismissBoxValue) {
        self.inner.anchored.animate_to(direction).await;
    }

    /// At `value` at once, as `snapTo`.
    pub fn snap_to(&self, value: SwipeToDismissBoxValue) {
        self.inner.anchored.snap_to(value);
    }
}

impl ComposeArg for SwipeToDismissBoxState {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

/// A swipe-to-dismiss state remembered here, as `rememberSwipeToDismissBoxState(initialValue,
/// confirmValueChange)`: a swipe of 56 dp dismisses (`SwipeToDismissBoxDefaults`'s positional
/// threshold).
pub fn remember_swipe_to_dismiss_box_state(
    initial_value: SwipeToDismissBoxValue,
    confirm_value_change: impl Fn(SwipeToDismissBoxValue) -> bool + 'static,
) -> SwipeToDismissBoxState {
    let threshold = compose_ui::LOCAL_DENSITY.current().to_px(Dp(56.0));
    remember(move || {
        SwipeToDismissBoxState::new(initial_value, confirm_value_change, move |_| threshold)
    })
}

/// A release faster than this dismisses, as Compose's `DismissThreshold` (125 dp/s).
const VELOCITY_THRESHOLD: Dp = Dp(125.0);

/// Content that can be swiped away sideways, as Compose's `SwipeToDismissBox`: dragged across,
/// it uncovers `background_content` (as large as the content, behind it), and let go past the
/// threshold or fast enough it settles away (`StartToEnd` or `EndToStart`, each if enabled), else
/// back in place. Remove the item when the state says it's dismissed (or in
/// `confirm_value_change`).
#[composable]
pub fn SwipeToDismissBox(
    state: SwipeToDismissBoxState,
    background_content: impl Fn(RowScope) + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(true)] enable_dismiss_from_start_to_end: bool,
    #[default(true)] enable_dismiss_from_end_to_start: bool,
    #[default(true)] gestures_enabled: bool,
    content: impl Fn(RowScope) + 'static,
) {
    let scope = remember_coroutine_scope();
    let density = compose_ui::LOCAL_DENSITY.current();
    let rtl = compose_ui::LOCAL_LAYOUT_DIRECTION.current() == LayoutDirection::Rtl;
    let drag = {
        let state = state.clone();
        compose_runtime::remember_keyed(state.clone(), move || {
            DraggableState::new(move |delta| state.inner.anchored.drag_by(delta))
        })
    };
    let settle = {
        let (state, scope) = (state.clone(), scope.clone());
        let threshold = density.to_px(VELOCITY_THRESHOLD);
        move |velocity: f32| {
            let state = state.clone();
            scope.launch(async move {
                let inner = &state.inner;
                inner.anchored.settle(velocity, &*inner.positional_threshold, threshold).await;
            });
        }
    };
    let policy = {
        let state = state.clone();
        compose_runtime::remember_keyed(
            (state.clone(), enable_dismiss_from_start_to_end, enable_dismiss_from_end_to_start),
            move || {
                Rc::new(SwipeLayout {
                    state,
                    start_to_end: enable_dismiss_from_start_to_end,
                    end_to_start: enable_dismiss_from_end_to_start,
                }) as Rc<dyn MeasurePolicy>
            },
        )
    };
    Layout(
        // Dragged only while in place, as Compose's (a dismissed item stays away); the offset is
        // logical (towards the end positive), so a right-to-left drag is reversed.
        modifier.draggable(
            drag,
            Orientation::Horizontal,
            gestures_enabled && state.current_value() == SwipeToDismissBoxValue::Settled,
            rtl,
            |_| {},
            settle,
        ),
        policy,
        compose_runtime::callback(move || {
            Row(
                Modifier,
                Arrangement::Start,
                Alignment::CENTER_VERTICALLY,
                background_content.clone(),
            );
            Row(Modifier, Arrangement::Start, Alignment::CENTER_VERTICALLY, content.clone());
        }),
    );
}

/// The background behind, as large as the content; the content at the state's (logical)
/// offset. The anchors: in place, and a width away each way enabled (towards the end positive,
/// placed mirrored in a right-to-left layout).
struct SwipeLayout {
    state: SwipeToDismissBoxState,
    start_to_end: bool,
    end_to_start: bool,
}

impl MeasurePolicy for SwipeLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let [background, content] = measurables else {
            return scope.layout(constraints.min_width, constraints.min_height);
        };
        let content = scope.measure(*content, constraints);
        let background =
            scope.measure(*background, Constraints::fixed(content.width, content.height));
        let width = content.width as f32;
        let towards_end = width;
        let mut anchors = vec![(SwipeToDismissBoxValue::Settled, 0.0)];
        if self.start_to_end {
            anchors.push((SwipeToDismissBoxValue::StartToEnd, towards_end));
        }
        if self.end_to_start {
            anchors.push((SwipeToDismissBoxValue::EndToStart, -towards_end));
        }
        let anchored = &self.state.inner.anchored;
        anchored.update_anchors(anchors);
        // Read while measuring: a frame of the swipe places the content again only.
        let offset = anchored.offset();
        let offset = if offset.is_nan() { 0 } else { offset.round() as i32 };
        scope.place(&background, 0, 0);
        // Mirrored in a right-to-left layout: towards the end is left there.
        scope.place_relative(&content, offset, 0);
        scope.layout(content.width, content.height)
    }

    fn debug_name(&self) -> &'static str {
        "SwipeToDismissBox"
    }
}
