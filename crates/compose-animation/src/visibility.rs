//! `AnimatedVisibility`, as Compose's: content that animates in when it becomes visible and out
//! before it leaves, with enter and exit transitions (fades, expansions, slides, scales)
//! combined with `+`.

use std::ops::Add;
use std::rc::Rc;

use compose_runtime::{SideEffect, State, composable, remember, remember_coroutine_scope};
use compose_ui::{
    Alignment, Constraints, IntSize, Layout, Measurable, MeasurePolicy, MeasureResult,
    MeasureScope, Modifier, RECTANGLE_SHAPE,
};

use crate::animatable::Animatable;
use crate::spec::{AnimationSpec, SpringSpec, spring};

/// Which axes an expansion or shrink changes.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Axes {
    Horizontal,
    Vertical,
    Both,
}

/// The parts of a transition: each animates on its own spec.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
struct Parts {
    /// The alpha at the hidden end, and the spec.
    fade: Option<(f32, AnimationSpec)>,
    /// The size fraction at the hidden end (0), the axes, which side stays, and the spec.
    size: Option<(Axes, Alignment, AnimationSpec)>,
    /// The offset at the hidden end, as fractions of the size, and the spec.
    slide: Option<((f32, f32), AnimationSpec)>,
    /// The scale at the hidden end, and the spec.
    scale: Option<(f32, AnimationSpec)>,
}

impl Parts {
    fn plus(self, other: Parts) -> Parts {
        Parts {
            fade: self.fade.or(other.fade),
            size: self.size.or(other.size),
            slide: self.slide.or(other.slide),
            scale: self.scale.or(other.scale),
        }
    }
}

/// How content appears, as Compose's `EnterTransition`: `fade_in() + expand_vertically()`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct EnterTransition(Parts);

/// How content leaves, as Compose's `ExitTransition`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ExitTransition(Parts);

impl EnterTransition {
    /// No animation, as `EnterTransition.None`.
    pub const NONE: EnterTransition =
        EnterTransition(Parts { fade: None, size: None, slide: None, scale: None });
}

impl ExitTransition {
    /// No animation, as `ExitTransition.None`.
    pub const NONE: ExitTransition =
        ExitTransition(Parts { fade: None, size: None, slide: None, scale: None });
}

impl Add for EnterTransition {
    type Output = EnterTransition;
    fn add(self, other: EnterTransition) -> EnterTransition {
        EnterTransition(self.0.plus(other.0))
    }
}

impl Add for ExitTransition {
    type Output = ExitTransition;
    fn add(self, other: ExitTransition) -> ExitTransition {
        ExitTransition(self.0.plus(other.0))
    }
}

impl compose_runtime::ComposeArg for EnterTransition {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl compose_runtime::ComposeArg for ExitTransition {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// Enter and exit transitions' default spring, as Compose's (`Spring.StiffnessMediumLow`).
fn default_spec() -> AnimationSpec {
    AnimationSpec::Spring(SpringSpec {
        stiffness: crate::Spring::STIFFNESS_MEDIUM_LOW,
        visibility_threshold: Some(0.001),
        ..spring()
    })
}

/// Fades in from `initial_alpha`, as `fadeIn()`.
pub fn fade_in() -> EnterTransition {
    fade_in_with(default_spec(), 0.0)
}

/// As `fadeIn(animationSpec, initialAlpha)`.
pub fn fade_in_with(spec: impl Into<AnimationSpec>, initial_alpha: f32) -> EnterTransition {
    EnterTransition(Parts { fade: Some((initial_alpha, spec.into())), ..Parts::default() })
}

/// Fades out to `target_alpha`, as `fadeOut()`.
pub fn fade_out() -> ExitTransition {
    fade_out_with(default_spec(), 0.0)
}

/// As `fadeOut(animationSpec, targetAlpha)`.
pub fn fade_out_with(spec: impl Into<AnimationSpec>, target_alpha: f32) -> ExitTransition {
    ExitTransition(Parts { fade: Some((target_alpha, spec.into())), ..Parts::default() })
}

/// Grows from no height, revealing from the bottom, as `expandVertically()`.
pub fn expand_vertically() -> EnterTransition {
    expand_with(Axes::Vertical, Alignment::BOTTOM_CENTER, default_spec())
}

/// Grows from no width, revealing from the end, as `expandHorizontally()`.
pub fn expand_horizontally() -> EnterTransition {
    expand_with(Axes::Horizontal, Alignment::CENTER_END, default_spec())
}

/// Grows from nothing, from the bottom end, as `expandIn()`.
pub fn expand_in() -> EnterTransition {
    expand_with(Axes::Both, Alignment::BOTTOM_END, default_spec())
}

/// Shrinks to no height, towards the bottom, as `shrinkVertically()`.
pub fn shrink_vertically() -> ExitTransition {
    ExitTransition(expand_with(Axes::Vertical, Alignment::BOTTOM_CENTER, default_spec()).0)
}

/// Shrinks to no width, towards the end, as `shrinkHorizontally()`.
pub fn shrink_horizontally() -> ExitTransition {
    ExitTransition(expand_with(Axes::Horizontal, Alignment::CENTER_END, default_spec()).0)
}

/// Shrinks to nothing, towards the bottom end, as `shrinkOut()`.
pub fn shrink_out() -> ExitTransition {
    ExitTransition(expand_with(Axes::Both, Alignment::BOTTOM_END, default_spec()).0)
}

fn expand_with(axes: Axes, from: Alignment, spec: AnimationSpec) -> EnterTransition {
    EnterTransition(Parts { size: Some((axes, from, spec)), ..Parts::default() })
}

/// Slides in from `initial_offset` (fractions of the content's size: `(0.0, -1.0)` from a full
/// height above), as `slideIn` / `slideInVertically`.
pub fn slide_in(initial_offset: (f32, f32)) -> EnterTransition {
    EnterTransition(Parts { slide: Some((initial_offset, default_spec())), ..Parts::default() })
}

/// Slides in from half its height above, as `slideInVertically()`.
pub fn slide_in_vertically() -> EnterTransition {
    slide_in((0.0, -0.5))
}

/// Slides out to `target_offset` (fractions of the content's size), as `slideOut`.
pub fn slide_out(target_offset: (f32, f32)) -> ExitTransition {
    ExitTransition(Parts { slide: Some((target_offset, default_spec())), ..Parts::default() })
}

/// Slides out to half its height above, as `slideOutVertically()`.
pub fn slide_out_vertically() -> ExitTransition {
    slide_out((0.0, -0.5))
}

/// Scales in from `initial_scale`, as `scaleIn()`.
pub fn scale_in(initial_scale: f32) -> EnterTransition {
    EnterTransition(Parts { scale: Some((initial_scale, default_spec())), ..Parts::default() })
}

/// Scales in from `initial_scale` along `spec`, as `scaleIn(animationSpec, initialScale)`.
pub fn scale_in_with(spec: impl Into<AnimationSpec>, initial_scale: f32) -> EnterTransition {
    EnterTransition(Parts { scale: Some((initial_scale, spec.into())), ..Parts::default() })
}

/// Scales out to `target_scale` along `spec`, as `scaleOut(animationSpec, targetScale)`.
pub fn scale_out_with(spec: impl Into<AnimationSpec>, target_scale: f32) -> ExitTransition {
    ExitTransition(Parts { scale: Some((target_scale, spec.into())), ..Parts::default() })
}

/// Scales out to `target_scale`, as `scaleOut()`.
pub fn scale_out(target_scale: f32) -> ExitTransition {
    ExitTransition(Parts { scale: Some((target_scale, default_spec())), ..Parts::default() })
}

/// The visibility animation's progress per part: 1 is shown, 0 hidden.
#[derive(Clone)]
struct Progress {
    alpha: Animatable<f32>,
    size: Animatable<f32>,
    slide: Animatable<f32>,
    scale: Animatable<f32>,
}

impl Progress {
    fn new(shown: bool) -> Progress {
        let at = if shown { 1.0 } else { 0.0 };
        Progress {
            alpha: Animatable::new(at),
            size: Animatable::new(at),
            slide: Animatable::new(at),
            scale: Animatable::new(at),
        }
    }
}

/// What the visibility layout does with its content: the axes and kept side of an expansion,
/// and a slide's offset, at the hidden end.
#[derive(Clone, Copy, PartialEq, Debug)]
struct LayoutParts {
    size: Option<(Axes, Alignment)>,
    slide: Option<(f32, f32)>,
}

impl compose_runtime::ComposeArg for LayoutParts {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// Measures its content at full size, takes the shown fraction of it on an expansion's axes
/// (keeping the side it names), and slides it by the hidden fraction of its offset: what
/// expansions, shrinks and slides show. The fractions are state read while measuring, so an
/// animation frame re-measures this node only.
struct VisibilityLayout {
    parts: LayoutParts,
    size_shown: State<f32>,
    slide_shown: State<f32>,
}

impl MeasurePolicy for VisibilityLayout {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let placeables: Vec<_> =
            measurables.iter().map(|&measurable| scope.measure(measurable, constraints)).collect();
        let full = placeables.iter().fold(
            IntSize::new(constraints.min_width, constraints.min_height),
            |size, placeable| {
                IntSize::new(size.width.max(placeable.width), size.height.max(placeable.height))
            },
        );
        let (size, alignment) = match self.parts.size {
            Some((axes, alignment)) => {
                let shown = self.size_shown.get().clamp(0.0, 1.0);
                let scaled = |length: i32| (length as f32 * shown).round() as i32;
                let width = if axes == Axes::Vertical { full.width } else { scaled(full.width) };
                let height =
                    if axes == Axes::Horizontal { full.height } else { scaled(full.height) };
                (constraints.constrain(IntSize::new(width, height)), alignment)
            }
            None => (full, Alignment::TOP_START),
        };
        let (dx, dy) = match self.parts.slide {
            Some((x, y)) => {
                let hidden = 1.0 - self.slide_shown.get().clamp(0.0, 1.0);
                (
                    (full.width as f32 * x * hidden).round() as i32,
                    (full.height as f32 * y * hidden).round() as i32,
                )
            }
            None => (0, 0),
        };
        // The content keeps its full size, placed so the side that stays is at the edge.
        let direction = scope.layout_direction();
        let position = alignment.align(full, size, direction);
        for placeable in &placeables {
            let offset = alignment.align(placeable.size(), full, direction);
            scope.place(placeable, position.x + offset.x + dx, position.y + offset.y + dy);
        }
        scope.layout(size.width, size.height)
    }

    fn debug_name(&self) -> &'static str {
        "VisibilityLayout"
    }
}

/// Shows `content` while `visible`, animating it in with `enter` and out with `exit`, as
/// Compose's `AnimatedVisibility`. While leaving it stays composed, and it is removed when the
/// exit ends. The animations write state read while drawing and measuring: their frames redraw
/// and re-measure this node, without recomposing.
#[composable]
#[allow(non_snake_case)]
pub fn AnimatedVisibility(
    visible: bool,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::fade_in() + crate::expand_in())] enter: EnterTransition,
    #[default(crate::shrink_out() + crate::fade_out())] exit: ExitTransition,
    #[default("AnimatedVisibility")] label: &'static str,
    content: impl Fn() + 'static,
) {
    let _ = label;
    AnimatedEnterExit(visible, visible, modifier, enter, exit, content);
}

/// `AnimatedVisibility`, first composed as `initially_visible` (hidden, a content that enters
/// at once), as Compose's `AnimatedVisibility(visibleState = MutableTransitionState(…))`.
#[composable]
#[allow(non_snake_case)]
pub(crate) fn AnimatedEnterExit(
    visible: bool,
    initially_visible: bool,
    modifier: Modifier,
    enter: EnterTransition,
    exit: ExitTransition,
    content: impl Fn() + 'static,
) {
    let progress = remember(|| Rc::new(Progress::new(initially_visible)));
    let scope = remember_coroutine_scope();
    // The visibility last animated to, set when its animations launch, after the pass.
    let launched = remember(|| Rc::new(std::cell::Cell::new(initially_visible)));
    // The exit is running: the content stays until it ends.
    let leaving = remember(|| compose_runtime::mutable_state_of(false));
    let parts = if visible { enter.0 } else { exit.0 };
    // Each launch's number: an animation of an earlier launch (an enter the exit interrupted)
    // doesn't end the content's stay.
    let launches = remember(|| Rc::new(std::cell::Cell::new(0u64)));
    {
        let (progress, launched, launches) = (progress.clone(), launched.clone(), launches.clone());
        SideEffect(move || {
            if launched.get() == visible {
                return;
            }
            // Gone (not leaving): an enter starts from its parts' hidden ends.
            let gone = visible && !leaving.get();
            // Back while leaving: each part springs back from where it is, as Compose's
            // `PostExit -> Visible` (its default spring); an exit interrupting an enter goes on
            // along the interruption spring unless its own spec is one.
            let returning = visible && leaving.get();
            // Per part: an exit interrupting that part's enter (still running) goes on along
            // the interruption spring, unless its own spec is one; a finished part keeps its.
            let respec = move |spec: AnimationSpec, running: bool| match spec {
                _ if returning => default_spec(),
                AnimationSpec::Spring(_) => spec,
                _ if running && !visible => AnimationSpec::Spring(spring()),
                _ => spec,
            };
            let part = |animatable: &Animatable<f32>, spec: Option<AnimationSpec>| {
                (animatable.clone(), spec.map(|spec| respec(spec, animatable.is_running())))
            };
            launched.set(visible);
            leaving.set(!visible);
            let launch = launches.get() + 1;
            launches.set(launch);
            let target = if visible { 1.0 } else { 0.0 };
            let animations = [
                part(&progress.alpha, parts.fade.map(|(_, spec)| spec)),
                part(&progress.size, parts.size.map(|(_, _, spec)| spec)),
                part(&progress.slide, parts.slide.map(|(_, spec)| spec)),
                part(&progress.scale, parts.scale.map(|(_, spec)| spec)),
            ];
            // The parts animate at once, each as a task. A part the transition doesn't animate
            // keeps its value while leaving (an exit without a shrink keeps the size), and is
            // shown at once when entering. The last animation to end lets the content go,
            // unless another launch took over meanwhile.
            let left = Rc::new(std::cell::Cell::new(0usize));
            for (animatable, spec) in animations {
                let Some(spec) = spec else {
                    if visible {
                        animatable.snap_to(1.0);
                    }
                    continue;
                };
                if gone {
                    animatable.snap_to(0.0);
                }
                left.set(left.get() + 1);
                let (left, launches) = (left.clone(), launches.clone());
                scope.launch(async move {
                    animatable.animate_to(target, spec).await;
                    left.set(left.get() - 1);
                    if left.get() == 0 && launches.get() == launch && !visible {
                        leaving.set(false);
                    }
                });
            }
            if left.get() == 0 && !visible {
                leaving.set(false);
            }
        });
    }
    // Shown, or leaving (or about to: the pass that hides it keeps it).
    let shown = visible || leaving.get() || launched.get() != visible;
    if shown {
        // Each part's hidden end, from the transition going on: the exit's while leaving.
        let fade = parts.fade.map(|(alpha, _)| alpha);
        let scale = parts.scale.map(|(scale, _)| scale);
        let layout_parts = LayoutParts {
            size: parts.size.map(|(axes, alignment, _)| (axes, alignment)),
            slide: parts.slide.map(|(offset, _)| offset),
        };
        let policy = {
            let progress = progress.clone();
            compose_runtime::remember_keyed(layout_parts, || {
                Rc::new(VisibilityLayout {
                    parts: layout_parts,
                    size_shown: progress.size.as_state(),
                    slide_shown: progress.slide.as_state(),
                }) as Rc<dyn MeasurePolicy>
            })
        };
        let layer = {
            let progress = progress.clone();
            compose_runtime::remember_keyed((fade, scale), || {
                Modifier.graphics_layer(move |layer| {
                    if let Some(hidden) = fade {
                        layer.alpha = hidden + (1.0 - hidden) * progress.alpha.value();
                    }
                    if let Some(hidden) = scale {
                        let scale = hidden + (1.0 - hidden) * progress.scale.value();
                        layer.scale_x = scale;
                        layer.scale_y = scale;
                    }
                })
            })
        };
        let modifier =
            if layout_parts.size.is_some() { modifier.clip(RECTANGLE_SHAPE) } else { modifier };
        let content = content.clone();
        Layout(modifier.then(layer), policy, compose_runtime::callback(move || content()));
    }
}
