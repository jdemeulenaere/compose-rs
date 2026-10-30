//! `CapturedAnimatedVisibility`: a visibility whose content goes at once when hidden, what leaves
//! being the last frame it drew.

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::{Callback, composable, remember};
use compose_ui::{
    Constraints, ContentCapture, IntSize, Layout, Measurable, MeasurePolicy, MeasureResult,
    MeasureScope, Modifier,
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
    let capture = remember(ContentCapture::new);
    let policy = remember(|| {
        Rc::new(CapturedMeasurePolicy { exiting: Cell::new(false), size: Cell::new(IntSize::ZERO) })
    });
    policy.exiting.set(is_exiting);
    let enter_exit =
        create_enter_exit_modifier(&child, active_enter, active_exit, None, "CapturedBuiltIn");
    let (drawn, kept) = (capture.clone(), capture);
    Layout(
        modifier.then(enter_exit).draw_behind(move |scope| {
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
/// `CapturedAnimatedEnterExitMeasurePolicy`.
struct CapturedMeasurePolicy {
    exiting: Cell<bool>,
    size: Cell<IntSize>,
}

impl MeasurePolicy for CapturedMeasurePolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        if self.exiting.get() {
            let size = self.size.get();
            return scope.layout(size.width, size.height);
        }
        let size = measure_stacked(scope, measurables, constraints);
        self.size.set(size);
        scope.layout(size.width, size.height)
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
