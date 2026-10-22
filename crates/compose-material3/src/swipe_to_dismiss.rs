//! `SwipeToDismissBox`, as Compose's Material 3: content swiped away sideways, uncovering a
//! background, with its `SwipeToDismissBoxState`.

use compose_runtime::{ObjectHandle, Owned};
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

impl SwipeToDismissBoxValue {
    fn from_key(key: u8) -> SwipeToDismissBoxValue {
        match key {
            0 => SwipeToDismissBoxValue::StartToEnd,
            1 => SwipeToDismissBoxValue::EndToStart,
            _ => SwipeToDismissBoxValue::Settled,
        }
    }
}

impl ComposeArg for SwipeToDismissBoxValue {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

struct SwipeInner {
    anchored: Anchored,
    positional_threshold: Box<dyn Fn(f32) -> f32>,
}

/// A swipe-to-dismiss box's state, as Compose's `SwipeToDismissBoxState`.
///
/// A `Copy` handle (D240): [`new`](Self::new) returns its owner, and
/// [`remember_swipe_to_dismiss_box_state`] keeps that in the composition.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SwipeToDismissBoxState {
    object: ObjectHandle<SwipeInner>,
}

impl SwipeToDismissBoxState {
    /// Its object, or `None` once its owner went (D240): then it does nothing, its reads
    /// giving the defaults.
    #[track_caller]
    fn inner(&self) -> Option<Rc<SwipeInner>> {
        self.object.get()
    }

    /// As `SwipeToDismissBoxState(initialValue, density, confirmValueChange,
    /// positionalThreshold)`: `confirm_value_change` can refuse a dismissal (the content comes
    /// back); `positional_threshold` gives, from the width, how far (pixels) a swipe must go.
    #[track_caller]
    pub fn new(
        initial_value: SwipeToDismissBoxValue,
        confirm_value_change: impl Fn(SwipeToDismissBoxValue) -> bool + 'static,
        positional_threshold: impl Fn(f32) -> f32 + 'static,
    ) -> Owned<SwipeToDismissBoxState> {
        Owned::new(
            SwipeInner {
                anchored: Anchored::new(
                    initial_value as u8,
                    Box::new(move |key| {
                        confirm_value_change(SwipeToDismissBoxValue::from_key(key))
                    }),
                    AnimationSpec::Spring(compose_animation::spring()),
                ),
                positional_threshold: Box::new(positional_threshold),
            },
            |object| SwipeToDismissBoxState { object },
        )
    }

    /// Where it last settled (a state read), as `currentValue`.
    pub fn current_value(&self) -> SwipeToDismissBoxValue {
        self.inner().map_or(SwipeToDismissBoxValue::Settled, |inner| {
            SwipeToDismissBoxValue::from_key(inner.anchored.current_value())
        })
    }

    /// Where it's going, or would settle if let go now (a state read), as `targetValue`.
    pub fn target_value(&self) -> SwipeToDismissBoxValue {
        self.inner().map_or(SwipeToDismissBoxValue::Settled, |inner| {
            SwipeToDismissBoxValue::from_key(inner.anchored.target_value())
        })
    }

    /// The content's offset, in pixels (a state read), as `requireOffset()`.
    pub fn require_offset(&self) -> f32 {
        self.inner().map_or(0.0, |inner| inner.anchored.offset())
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
        let Some(inner) = self.inner() else { return 0.0 };
        let anchored = &inner.anchored;
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
        if let Some(inner) = self.inner() {
            inner.anchored.animate_to(SwipeToDismissBoxValue::Settled as u8).await;
        }
    }

    /// Animates it away in `direction`, as `dismiss(direction)`.
    pub async fn dismiss(&self, direction: SwipeToDismissBoxValue) {
        if let Some(inner) = self.inner() {
            inner.anchored.animate_to(direction as u8).await;
        }
    }

    /// At `value` at once, as `snapTo`.
    pub fn snap_to(&self, value: SwipeToDismissBoxValue) {
        if let Some(inner) = self.inner() {
            inner.anchored.snap_to(value as u8);
        }
    }
}

impl ComposeArg for SwipeToDismissBoxState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A swipe-to-dismiss state remembered here, as `rememberSwipeToDismissBoxState(initialValue,
/// confirmValueChange)`: a swipe of 56 dp dismisses (`SwipeToDismissBoxDefaults`'s positional
/// threshold).
#[composable(non_restartable)]
pub fn remember_swipe_to_dismiss_box_state(
    #[default(crate::SwipeToDismissBoxValue::Settled)] initial_value: SwipeToDismissBoxValue,
    #[default(|_| true)] confirm_value_change: impl Fn(SwipeToDismissBoxValue) -> bool + 'static,
) -> SwipeToDismissBoxState {
    let threshold = compose_ui::LOCAL_DENSITY.current().to_px(Dp(56.0));
    remember(move || {
        SwipeToDismissBoxState::new(
            initial_value,
            move |value| confirm_value_change(value),
            move |_| threshold,
        )
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
        compose_runtime::remember_keyed(state, move || {
            DraggableState::new(move |delta| {
                if let Some(inner) = state.inner() {
                    inner.anchored.drag_by(delta);
                }
            })
        })
    };
    let settle = {
        let threshold = density.to_px(VELOCITY_THRESHOLD);
        move |velocity: f32| {
            scope.launch(async move {
                let Some(inner) = state.inner() else { return };
                inner.anchored.settle(velocity, &*inner.positional_threshold, threshold).await;
            });
        }
    };
    let policy = {
        compose_runtime::remember_keyed(
            (state, enable_dismiss_from_start_to_end, enable_dismiss_from_end_to_start),
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
        let Some(inner) = self.state.inner() else {
            scope.place(&background, 0, 0);
            scope.place_relative(&content, 0, 0);
            return scope.layout(content.width, content.height);
        };
        let anchored = &inner.anchored;
        anchored.update_anchors(anchors.into_iter().map(|(value, at)| (value as u8, at)).collect());
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
