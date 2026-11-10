//! `CapturedAnimatedVisibility`: a visibility whose content goes at once when hidden, what leaves
//! being the last frame it drew.

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::{Callback, composable, remember};
use compose_ui::{
    Constraints, DrawCapture, IntSize, IntrinsicMeasurable, IntrinsicMeasureScope, Layout,
    Measurable, MeasurePolicy, MeasureResult, MeasureScope, Modifier,
};

use crate::transition::Transition;
use crate::visibility::{
    AnimatedVisibilityState, EnterExitState, EnterTransition, ExitTransition,
    create_enter_exit_modifier, target_enter_exit, update_enter_exit_modifier_state,
};

/// Shows `content` while `visible` (a `bool`, or a `MutableTransitionState` to see where it's at),
/// as `CapturedAnimatedVisibility`: unlike [`AnimatedVisibility`](crate::AnimatedVisibility), when
/// it's hidden the content leaves the composition at once, and `exit` animates the last frame it
/// drew, which keeps no state and takes no input. So it has no scope, and an interrupted exit
/// composes the content anew. `enter` and `exit` default to the state's.
#[composable]
#[allow(non_snake_case)]
pub fn CapturedAnimatedVisibility<V: AnimatedVisibilityState>(
    visible: V,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] enter: impl Into<Option<EnterTransition>>,
    #[default(None)] exit: impl Into<Option<ExitTransition>>,
    #[default("CapturedAnimatedVisibility")] label: &'static str,
    content: impl Fn() + 'static,
) {
    let enter: Option<EnterTransition> = enter;
    let enter = enter.unwrap_or_else(|| visible.default_enter());
    let exit: Option<ExitTransition> = exit;
    let exit = exit.unwrap_or_else(|| visible.default_exit());
    CapturedEnterExit(
        visible.transition(label),
        modifier,
        enter,
        exit,
        compose_runtime::callback(content),
    );
}

#[composable(non_restartable)]
#[allow(non_snake_case)]
fn CapturedEnterExit(
    transition: Transition<bool>,
    modifier: Modifier,
    enter: EnterTransition,
    exit: ExitTransition,
    content: Callback<dyn Fn()>,
) {
    if !transition.target_state() && !transition.current_state() {
        return;
    }
    let child = {
        let parent = transition.clone();
        let visible: crate::visibility::VisibleFn<bool> = Rc::new(|visible: &bool| *visible);
        transition.create_child_transition("EnterExitTransition", move |state| {
            target_enter_exit(&parent, &visible, state)
        })
    };
    let (active_enter, active_exit, _) =
        update_enter_exit_modifier_state(&child, enter, exit, None);
    let is_exiting = child.target_state() == EnterExitState::PostExit;
    let capture = remember(DrawCapture::new);
    let policy = remember(|| {
        Rc::new(CapturedMeasurePolicy {
            exiting: Cell::new(false),
            size: Cell::new(IntSize::ZERO),
            lookahead_size: Cell::new(None),
        })
    });
    policy.exiting.set(is_exiting);
    let enter_exit = create_enter_exit_modifier(
        &child,
        active_enter,
        active_exit,
        None,
        false,
        "CapturedBuiltIn",
    );
    // Ahead, in a `LookaheadScope`, no size once exiting: what's around it is laid out ahead as
    // it ends, as Compose's `CapturedAnimatedVisibilityImpl`'s layout (D2352).
    let no_size_ahead_when_exiting = Modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        let (width, height) = if is_exiting && scope.is_looking_ahead() {
            (0, 0)
        } else {
            (placeable.width, placeable.height)
        };
        scope.layout_with(width, height, move |placement| placement.place(&placeable, 0, 0))
    });
    let (drawn, kept) = (capture.clone(), capture);
    Layout(
        modifier.then(no_size_ahead_when_exiting).then(enter_exit).draw_behind(move |scope| {
            if is_exiting {
                scope.draw_captured(&drawn);
            }
        }),
        policy,
        compose_runtime::callback(move || {
            if !is_exiting {
                // A node of its own: what's captured is the content's, not what moves it.
                let captured = kept.clone();
                Layout(
                    Modifier.capture_content(captured),
                    remember(|| Rc::new(StackedPolicy) as Rc<dyn MeasurePolicy>),
                    content.clone(),
                );
            }
        }),
    );
}

/// Its size is the content's, kept for when the content's gone, as
/// `CapturedAnimatedEnterExitMeasurePolicy`: the size of now and, in a `LookaheadScope`, the size
/// of ahead, each kept by its own pass.
struct CapturedMeasurePolicy {
    exiting: Cell<bool>,
    size: Cell<IntSize>,
    /// The content's size ahead, as `lookaheadSize`: none until measured ahead.
    lookahead_size: Cell<Option<IntSize>>,
}

impl CapturedMeasurePolicy {
    /// The size kept by the pass `looking_ahead` says, as `lookaheadSize` and `approachSize`.
    /// None measured ahead yet (Compose's check fails there), the size of now.
    fn kept(&self, looking_ahead: bool) -> IntSize {
        let now = self.size.get();
        if looking_ahead { self.lookahead_size.get().unwrap_or(now) } else { now }
    }

    /// The kept size's while exiting, else the largest of the children's: not by measuring, which
    /// would take the kept size away.
    fn intrinsic(
        &self,
        kept: i32,
        measurables: &[IntrinsicMeasurable],
        mut of: impl FnMut(IntrinsicMeasurable) -> i32,
    ) -> i32 {
        if self.exiting.get() {
            return kept;
        }
        measurables.iter().map(|&measurable| of(measurable)).max().unwrap_or(0)
    }
}

impl MeasurePolicy for CapturedMeasurePolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let looking_ahead = scope.is_looking_ahead();
        if self.exiting.get() {
            let size = self.kept(looking_ahead);
            return scope.layout(size.width, size.height);
        }
        let size = measure_stacked(scope, measurables, constraints);
        if looking_ahead {
            self.lookahead_size.set(Some(size));
        } else {
            self.size.set(size);
        }
        scope.layout(size.width, size.height)
    }

    fn min_intrinsic_width(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        let kept = self.kept(scope.is_looking_ahead()).width;
        self.intrinsic(kept, measurables, |measurable| {
            scope.min_intrinsic_width(measurable, height)
        })
    }

    fn min_intrinsic_height(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        let kept = self.kept(scope.is_looking_ahead()).height;
        self.intrinsic(kept, measurables, |measurable| {
            scope.min_intrinsic_height(measurable, width)
        })
    }

    fn max_intrinsic_width(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        height: i32,
    ) -> i32 {
        let kept = self.kept(scope.is_looking_ahead()).width;
        self.intrinsic(kept, measurables, |measurable| {
            scope.max_intrinsic_width(measurable, height)
        })
    }

    fn max_intrinsic_height(
        &self,
        scope: &mut IntrinsicMeasureScope<'_>,
        measurables: &[IntrinsicMeasurable],
        width: i32,
    ) -> i32 {
        let kept = self.kept(scope.is_looking_ahead()).height;
        self.intrinsic(kept, measurables, |measurable| {
            scope.max_intrinsic_height(measurable, width)
        })
    }

    fn debug_name(&self) -> &'static str {
        "CapturedAnimatedEnterExit"
    }
}

/// The content's children, stacked at the top start, as large as the largest.
struct StackedPolicy;

impl MeasurePolicy for StackedPolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let size = measure_stacked(scope, measurables, constraints);
        scope.layout(size.width, size.height)
    }
}

fn measure_stacked(
    scope: &mut MeasureScope<'_>,
    measurables: &[Measurable],
    constraints: Constraints,
) -> IntSize {
    let placeables: Vec<_> =
        measurables.iter().map(|&measurable| scope.measure(measurable, constraints)).collect();
    for placeable in &placeables {
        scope.place(placeable, 0, 0);
    }
    placeables.iter().fold(IntSize::ZERO, |size, placeable| {
        IntSize::new(size.width.max(placeable.width), size.height.max(placeable.height))
    })
}
