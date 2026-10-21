//! `remember_infinite_transition`, as Compose's `rememberInfiniteTransition`: values that animate
//! forever while in the composition (a pulsing badge, a spinner).

use compose_runtime::{ComposeArg, LaunchedEffect, State, composable, remember};
use compose_ui::Color;

use crate::animatable::Animatable;
use crate::spec::RepeatableSpec;
use crate::vector::Animate;

/// Runs infinite animations while it is in the composition, as Compose's `InfiniteTransition`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InfiniteTransition {
    _private: (),
}

/// An infinite transition, as `rememberInfiniteTransition()`.
#[composable]
pub fn remember_infinite_transition(
    #[default("InfiniteTransition")] label: &'static str,
) -> InfiniteTransition {
    let _ = label;
    InfiniteTransition { _private: () }
}

impl InfiniteTransition {
    /// A value going from `initial_value` to `target_value` along `animation_spec` forever, as
    /// `InfiniteTransition.animateValue`. Changing the values or the spec restarts it.
    pub fn animate_value<T: Animate + ComposeArg>(
        &self,
        initial_value: T,
        target_value: T,
        animation_spec: RepeatableSpec,
    ) -> State<T> {
        let animatable = remember(|| Animatable::new(initial_value.clone()));
        let state = animatable.as_state();
        let keys = (initial_value.clone(), target_value.clone(), SpecKey(animation_spec));
        LaunchedEffect(keys, async move {
            animatable.snap_to(initial_value);
            animatable.animate_to(target_value, animation_spec).await;
        });
        state
    }

    /// As `InfiniteTransition.animateFloat`.
    pub fn animate_float(
        &self,
        initial_value: f32,
        target_value: f32,
        animation_spec: RepeatableSpec,
    ) -> State<f32> {
        self.animate_value(initial_value, target_value, animation_spec)
    }

    /// As `InfiniteTransition.animateColor`.
    pub fn animate_color(
        &self,
        initial_value: Color,
        target_value: Color,
        animation_spec: RepeatableSpec,
    ) -> State<Color> {
        self.animate_value(initial_value, target_value, animation_spec)
    }
}

/// A spec compared by value, for the effect's keys.
#[derive(Clone, Copy, PartialEq, Debug)]
struct SpecKey(RepeatableSpec);

impl ComposeArg for SpecKey {
    fn same(&self, other: &SpecKey) -> bool {
        self == other
    }
}
