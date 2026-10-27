//! `CircularProgressIndicator` and `LinearProgressIndicator`, as Compose's Material 3 (1.3 on,
//! at androidx-main's tip): determinate (a progress from 0 to 1, read from a lambda while
//! drawing) or indeterminate (animated forever), a gap between the indicator and its track, and
//! a stop dot at a linear track's end. Their animations are read while drawing, so each frame
//! only draws again.

use std::f32::consts::PI;

use compose_animation::{
    Easing, RepeatMode, SpringSpec, infinite_repeatable, remember_infinite_transition, tween,
};
use compose_runtime::{Callback, State, composable};
use compose_ui::{
    Color, Dp, DrawScope, Modifier, ModifierNode, ModifierNodeElement, Offset, Paint,
    ProgressBarRangeInfo, Rect, SemanticsConfiguration, SemanticsModifierNode, StrokeCap,
};

use crate::components::increase_semantics_bounds;
use crate::theme::MaterialTheme;

/// As Compose's `ProgressIndicatorDefaults`.
pub struct ProgressIndicatorDefaults;

impl ProgressIndicatorDefaults {
    /// The stroke of a circular indicator, as `CircularStrokeWidth`.
    pub const CIRCULAR_STROKE_WIDTH: Dp = Dp(4.0);
    /// The size of a circular indicator, as `CircularIndicatorDiameter`.
    pub const CIRCULAR_INDICATOR_DIAMETER: Dp = Dp(40.0);
    /// A linear indicator's default width, as `LinearIndicatorWidth`.
    pub const LINEAR_INDICATOR_WIDTH: Dp = Dp(240.0);
    /// A linear indicator's height, as `LinearIndicatorHeight`.
    pub const LINEAR_INDICATOR_HEIGHT: Dp = Dp(4.0);
    /// Between a linear indicator's line and its track, as `LinearIndicatorTrackGapSize`
    /// (`LinearProgressIndicatorTokens.TrackActiveSpace`).
    pub const LINEAR_INDICATOR_TRACK_GAP_SIZE: Dp = Dp(4.0);
    /// Between a circular indicator's arc and its track, as `CircularIndicatorTrackGapSize`
    /// (`CircularProgressIndicatorTokens.TrackActiveSpace`).
    pub const CIRCULAR_INDICATOR_TRACK_GAP_SIZE: Dp = Dp(4.0);
    /// The stop dot at a linear track's end, as `LinearTrackStopIndicatorSize`
    /// (`LinearProgressIndicatorTokens.StopSize`).
    pub const LINEAR_TRACK_STOP_INDICATOR_SIZE: Dp = Dp(4.0);
    /// A linear indicator's ends, as `LinearStrokeCap`: `Round` since Material 3 1.3 (`Butt`
    /// before).
    pub const LINEAR_STROKE_CAP: StrokeCap = StrokeCap::Round;
    /// A determinate circular indicator's ends, as `CircularDeterminateStrokeCap`: `Round`
    /// since Material 3 1.3 (`Butt` before).
    pub const CIRCULAR_DETERMINATE_STROKE_CAP: StrokeCap = StrokeCap::Round;
    /// An indeterminate circular indicator's ends, as `CircularIndeterminateStrokeCap`: `Round`
    /// since Material 3 1.3 (`Square` before). A round arc is also one Skia draws directly
    /// (`CircleOp`), where a square-capped one becomes a stroked path each frame.
    pub const CIRCULAR_INDETERMINATE_STROKE_CAP: StrokeCap = StrokeCap::Round;
    /// The spring Compose recommends for animating a progress, as `ProgressAnimationSpec`: no
    /// bounce, very low stiffness, a thousandth of the range as its threshold.
    pub const PROGRESS_ANIMATION_SPEC: SpringSpec = SpringSpec {
        damping_ratio: compose_animation::Spring::DAMPING_RATIO_NO_BOUNCY,
        stiffness: compose_animation::Spring::STIFFNESS_VERY_LOW,
        visibility_threshold: Some(compose_animation::AnimationVector::filled(1, 1.0 / 1000.0)),
    };

    /// A linear indicator's color, as `linearColor`.
    pub fn linear_color() -> Color {
        MaterialTheme::color_scheme().primary
    }

    /// A circular indicator's color, as `circularColor`.
    pub fn circular_color() -> Color {
        MaterialTheme::color_scheme().primary
    }

    /// A linear indicator's track, as `linearTrackColor`: the secondary container
    /// (`ProgressIndicatorTokens.TrackColor`, Material 3 1.3 on; the surface variant before).
    pub fn linear_track_color() -> Color {
        MaterialTheme::color_scheme().secondary_container
    }

    /// A determinate circular indicator's track, as `circularDeterminateTrackColor`: the
    /// secondary container.
    pub fn circular_determinate_track_color() -> Color {
        MaterialTheme::color_scheme().secondary_container
    }

    /// An indeterminate circular indicator's track, as `circularIndeterminateTrackColor`: none.
    pub fn circular_indeterminate_track_color() -> Color {
        Color::TRANSPARENT
    }

    /// Draws a linear indicator's stop dot at its track's end, as `drawStopIndicator`: a
    /// circle (round caps) or a square `stop_size` wide (at most the track's height), kept
    /// from the end by as much as it's thinner than the track (at most 6 dp), at the left in a
    /// right-to-left layout.
    pub fn draw_stop_indicator(
        scope: &mut DrawScope<'_>,
        stop_size: Dp,
        color: Color,
        stroke_cap: StrokeCap,
    ) {
        let size = scope.size();
        let stop = scope.density().to_px(stop_size).min(size.height);
        let max_offset = scope.density().to_px(STOP_INDICATOR_TRAILING_SPACE);
        let offset = ((size.height - stop) / 2.0).min(max_offset);
        // From the end: the right, or the left in a right-to-left layout (Compose flips it
        // with a scale of -1).
        let rtl = scope.layout_direction() == compose_ui::LayoutDirection::Rtl;
        let end = |x: f32| if rtl { size.width - x } else { x };
        if stroke_cap == StrokeCap::Round {
            let center = Offset::new(end(size.width - stop / 2.0 - offset), size.height / 2.0);
            scope.draw_circle(center, stop / 2.0, Paint::fill(color));
        } else {
            let (a, b) = (end(size.width - stop - offset), end(size.width - offset));
            let top = (size.height - stop) / 2.0;
            scope.draw_rect(Rect::new(a.min(b), top, a.max(b), top + stop), Paint::fill(color));
        }
    }
}

/// How far the stop dot keeps from the track's end at most, as `StopIndicatorTrailingSpace`.
const STOP_INDICATOR_TRAILING_SPACE: Dp = Dp(6.0);

/// A progress coerced between 0 and 1, NaN taken as 0: what Compose's indicators report
/// (`coercedProgress().takeUnless { it.isNaN() } ?: 0f`) and, here, draw.
fn coerce_progress(progress: f32) -> f32 {
    if progress.is_nan() { 0.0 } else { progress.clamp(0.0, 1.0) }
}

/// An indicator's progress semantics: the progress lambda's value (read when the semantics are,
/// as Compose reads it in its semantics block), or indeterminate.
struct ProgressSemantics {
    progress: Option<Callback<dyn Fn() -> f32>>,
}

impl ModifierNodeElement for ProgressSemantics {
    type Node = ProgressSemanticsNode;
    fn create(&self) -> ProgressSemanticsNode {
        ProgressSemanticsNode { progress: self.progress.clone() }
    }
    fn update(&self, node: &mut ProgressSemanticsNode) {
        node.progress = self.progress.clone();
    }
    fn same(&self, other: &Self) -> bool {
        match (&self.progress, &other.progress) {
            (Some(a), Some(b)) => a.ptr_eq(b),
            (None, None) => true,
            _ => false,
        }
    }
    fn name(&self) -> &'static str {
        "progress_semantics"
    }
}

struct ProgressSemanticsNode {
    progress: Option<Callback<dyn Fn() -> f32>>,
}

impl ModifierNode for ProgressSemanticsNode {
    fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
        Some(self)
    }
}

impl SemanticsModifierNode for ProgressSemanticsNode {
    fn apply_semantics(&self, config: &mut SemanticsConfiguration) {
        config.set_progress_bar_range_info(match &self.progress {
            Some(progress) => ProgressBarRangeInfo {
                current: coerce_progress(progress()),
                range: (0.0, 1.0),
                steps: 0,
            },
            // Indeterminate, as `ProgressBarRangeInfo.Indeterminate`.
            None => ProgressBarRangeInfo { current: 0.0, range: (0.0, 0.0), steps: 0 },
        });
    }
}

/// An indicator's node merging what's inside, with its progress semantics.
fn progress_semantics(modifier: Modifier, progress: Option<Callback<dyn Fn() -> f32>>) -> Modifier {
    modifier.semantics_with(true, |_| {}).then_custom(ProgressSemantics { progress })
}

/// The gap between a circular indicator's arc and its track, in degrees: `gap_size`, grown by
/// the stroke for capped ends (their caps reach half a stroke past each end), as a fraction of
/// the circumference, as Compose's `gapSizeSweep`.
fn circular_gap_sweep(
    scope: &DrawScope<'_>,
    gap_size: Dp,
    stroke_width: Dp,
    cap: StrokeCap,
) -> f32 {
    let size = scope.size();
    let adjusted = if cap == StrokeCap::Butt || size.height > size.width {
        gap_size
    } else {
        gap_size + stroke_width
    };
    let width = size.width / scope.density().density;
    if width <= 0.0 {
        return 0.0;
    }
    adjusted.0 / (PI * width) * 360.0
}

/// A determinate circular indicator, as Compose's `CircularProgressIndicator(progress: () ->
/// Float, …)`: an arc from the top, clockwise, `progress()` (0 to 1) of the way round, then
/// `gap_size` and the track (in `track_color`) to its start. `progress` is read while drawing
/// and when the semantics are read, so a state it reads draws again without composing.
#[composable]
pub fn CircularProgressIndicator(
    progress: impl Fn() -> f32 + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::ProgressIndicatorDefaults::circular_color())] color: Color,
    #[default(crate::ProgressIndicatorDefaults::CIRCULAR_STROKE_WIDTH)] stroke_width: Dp,
    #[default(crate::ProgressIndicatorDefaults::circular_determinate_track_color())]
    track_color: Color,
    #[default(crate::ProgressIndicatorDefaults::CIRCULAR_DETERMINATE_STROKE_CAP)]
    stroke_cap: StrokeCap,
    #[default(crate::ProgressIndicatorDefaults::CIRCULAR_INDICATOR_TRACK_GAP_SIZE)] gap_size: Dp,
) {
    let modifier = progress_semantics(modifier, Some(progress.clone()))
        .size(ProgressIndicatorDefaults::CIRCULAR_INDICATOR_DIAMETER);
    compose_foundation::layout::Canvas(modifier, move |scope| {
        let stroke = scope.density().to_px(stroke_width);
        let oval = inset_oval(scope.size().width.min(scope.size().height), stroke);
        // From the top (12 o'clock).
        let start = 270.0;
        let sweep = coerce_progress(progress()) * 360.0;
        let gap = sweep.min(circular_gap_sweep(scope, gap_size, stroke_width, stroke_cap));
        if track_color.alpha() > 0 {
            scope.draw_arc(
                oval,
                start + sweep + gap,
                360.0 - sweep - gap * 2.0,
                Paint::stroke_with(track_color, stroke, stroke_cap),
            );
        }
        scope.draw_arc(oval, start, sweep, Paint::stroke_with(color, stroke, stroke_cap));
    });
}

/// An indeterminate circular indicator, as Compose's `CircularProgressIndicator()` (Material 3
/// 1.3 on): an arc growing from a tenth of the circle to 87 % and back over 6 s while the whole
/// turns three times, jumping a further quarter turn every 1.5 s, its track (none by default)
/// past `gap_size`.
#[composable]
pub fn IndeterminateCircularProgressIndicator(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::ProgressIndicatorDefaults::circular_color())] color: Color,
    #[default(crate::ProgressIndicatorDefaults::CIRCULAR_STROKE_WIDTH)] stroke_width: Dp,
    #[default(crate::ProgressIndicatorDefaults::circular_indeterminate_track_color())]
    track_color: Color,
    #[default(crate::ProgressIndicatorDefaults::CIRCULAR_INDETERMINATE_STROKE_CAP)]
    stroke_cap: StrokeCap,
    #[default(crate::ProgressIndicatorDefaults::CIRCULAR_INDICATOR_TRACK_GAP_SIZE)] gap_size: Dp,
) {
    // One clock for the 6 s cycle, the rotations and the progress from it as Compose's three
    // animations (`circularIndeterminateGlobalRotationAnimationSpec`, `…RotationAnimationSpec`,
    // `…ProgressAnimationSpec`), which share its period.
    let transition = remember_infinite_transition("CircularProgressIndicator");
    let cycle = CIRCULAR_CYCLE as f32;
    let time: State<f32> = transition.animate_float(
        0.0,
        cycle,
        infinite_repeatable(tween(CIRCULAR_CYCLE).easing(Easing::Linear), RepeatMode::Restart),
    );
    // A repaint boundary of its own: it draws again every frame, what's around it needn't
    // (D323).
    let modifier = progress_semantics(modifier, None)
        .size(ProgressIndicatorDefaults::CIRCULAR_INDICATOR_DIAMETER)
        .repaint_boundary();
    compose_foundation::layout::Canvas(modifier, move |scope| {
        let stroke = scope.density().to_px(stroke_width);
        let size = scope.size().width.min(scope.size().height);
        let oval = inset_oval(size, stroke);
        let (rotation, progress) = circular_indeterminate_state(time.get());
        let sweep = progress * 360.0;
        let gap = sweep.min(circular_gap_sweep(scope, gap_size, stroke_width, stroke_cap));
        // Compose rotates the canvas about the center; the arcs' start angles turn instead.
        if track_color.alpha() > 0 {
            scope.draw_arc(
                oval,
                rotation + sweep + gap,
                360.0 - sweep - gap * 2.0,
                Paint::stroke_with(track_color, stroke, stroke_cap),
            );
        }
        scope.draw_arc(oval, rotation, sweep, Paint::stroke_with(color, stroke, stroke_cap));
    });
}

/// The indeterminate circular cycle, as `CircularAnimationProgressDuration`.
const CIRCULAR_CYCLE: u32 = 6000;
/// The turns of a cycle, as `CircularGlobalRotationDegreesTarget`.
const CIRCULAR_GLOBAL_ROTATION: f32 = 1080.0;
/// A quarter-turn jump's duration and the time between jumps, as
/// `CircularAnimationAdditionalRotationDuration` and `…Delay`.
const CIRCULAR_JUMP_DURATION: f32 = 300.0;
const CIRCULAR_JUMP_PERIOD: f32 = 1500.0;
/// The arc's shortest and longest, as `CircularIndeterminateMinProgress` and `…MaxProgress`.
const CIRCULAR_MIN_PROGRESS: f32 = 0.1;
const CIRCULAR_MAX_PROGRESS: f32 = 0.87;
/// The arc's shrinking, as `CircularProgressEasing` (`MotionTokens.EasingStandardCubicBezier`).
const CIRCULAR_PROGRESS_EASING: Easing = Easing::cubic_bezier(0.2, 0.0, 0.0, 1.0);

/// The indeterminate circular indicator's rotation (degrees) and progress (a fraction of the
/// circle) `time` milliseconds into its cycle, as Compose's keyframes: the global turn, linear;
/// a quarter turn more in the first 300 ms of each 1.5 s (linear: the keyframes' easing is
/// their flat segment's); the progress growing linearly to 87 % in 3 s, then shrinking back
/// to 10 %, eased.
fn circular_indeterminate_state(time: f32) -> (f32, f32) {
    let time = time.clamp(0.0, CIRCULAR_CYCLE as f32);
    let global = time / CIRCULAR_CYCLE as f32 * CIRCULAR_GLOBAL_ROTATION;
    let jumps = (time / CIRCULAR_JUMP_PERIOD).floor();
    let into = time - jumps * CIRCULAR_JUMP_PERIOD;
    let additional = 90.0 * (jumps + (into / CIRCULAR_JUMP_DURATION).min(1.0));
    let half = CIRCULAR_CYCLE as f32 / 2.0;
    let progress = if time <= half {
        CIRCULAR_MIN_PROGRESS + (CIRCULAR_MAX_PROGRESS - CIRCULAR_MIN_PROGRESS) * time / half
    } else {
        let eased = CIRCULAR_PROGRESS_EASING.transform((time - half) / half);
        CIRCULAR_MAX_PROGRESS + (CIRCULAR_MIN_PROGRESS - CIRCULAR_MAX_PROGRESS) * eased
    };
    ((global + additional.min(360.0)) % 360.0, progress)
}

/// The square `size` wide, inset by half the stroke, its arc drawn in.
fn inset_oval(size: f32, stroke: f32) -> Rect {
    let half = stroke / 2.0;
    Rect::new(half, half, size - half, size - half)
}

/// A determinate linear indicator, as Compose's `LinearProgressIndicator(progress: () -> Float,
/// …)`: a line from the start, `progress()` (0 to 1) of the way, then `gap_size` and the track
/// to the end, and the stop dot at the track's end; 240 by 4 dp. A capped line's gap grows by
/// the stroke, so its caps don't cross into it (a gap of 0: the line's and the track's round
/// ends touch). `draw_stop_indicator` draws the stop dot: `None` draws Compose's default
/// (`ProgressIndicatorDefaults::draw_stop_indicator` in `color` and `stroke_cap`),
/// `Some(DrawStopIndicator::none())` none. `progress` is read while drawing and when the semantics are read.
#[composable]
pub fn LinearProgressIndicator(
    progress: impl Fn() -> f32 + 'static,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::ProgressIndicatorDefaults::linear_color())] color: Color,
    #[default(crate::ProgressIndicatorDefaults::linear_track_color())] track_color: Color,
    #[default(crate::ProgressIndicatorDefaults::LINEAR_STROKE_CAP)] stroke_cap: StrokeCap,
    #[default(crate::ProgressIndicatorDefaults::LINEAR_INDICATOR_TRACK_GAP_SIZE)] gap_size: Dp,
    #[default(None)] draw_stop_indicator: Option<DrawStopIndicator>,
) {
    let modifier = increase_semantics_bounds(modifier, false);
    let modifier = linear_size(progress_semantics(modifier, Some(progress.clone())));
    compose_foundation::layout::Canvas(modifier, move |scope| {
        let progress = coerce_progress(progress());
        let track_start = progress + progress.min(linear_gap_fraction(scope, gap_size, stroke_cap));
        if track_start <= 1.0 {
            draw_line_fraction(scope, track_start, 1.0, track_color, stroke_cap);
        }
        draw_line_fraction(scope, 0.0, progress, color, stroke_cap);
        match &draw_stop_indicator {
            Some(draw) => (draw.0)(scope),
            None => ProgressIndicatorDefaults::draw_stop_indicator(
                scope,
                ProgressIndicatorDefaults::LINEAR_TRACK_STOP_INDICATOR_SIZE,
                color,
                stroke_cap,
            ),
        }
    });
}

/// What draws a linear indicator's stop dot, as Compose's `drawStopIndicator: DrawScope.() ->
/// Unit` (a type of its own: a composable's callbacks take their arguments by value). Compared
/// by identity.
#[derive(Clone)]
pub struct DrawStopIndicator(pub std::rc::Rc<dyn Fn(&mut DrawScope<'_>)>);

impl DrawStopIndicator {
    pub fn new(draw: impl Fn(&mut DrawScope<'_>) + 'static) -> DrawStopIndicator {
        DrawStopIndicator(std::rc::Rc::new(draw))
    }

    /// Draws nothing, as `drawStopIndicator = {}`: one per thread, so a call passing it can
    /// skip.
    pub fn none() -> DrawStopIndicator {
        thread_local! {
            static NONE: DrawStopIndicator = DrawStopIndicator::new(|_| {});
        }
        NONE.with(Clone::clone)
    }
}

impl compose_runtime::ComposeArg for DrawStopIndicator {
    fn same(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }
}

/// The gap between a linear indicator's line and its track, as a fraction of its width:
/// `gap_size`, and the stroke's width for a capped line (its caps reach half a stroke past each
/// end), as Compose's `adjustedGapSize`.
fn linear_gap_fraction(scope: &DrawScope<'_>, gap_size: Dp, cap: StrokeCap) -> f32 {
    let size = scope.size();
    if size.width <= 0.0 {
        return 0.0;
    }
    let gap = scope.density().to_px(gap_size);
    let adjusted =
        if cap == StrokeCap::Butt || size.height > size.width { gap } else { gap + size.height };
    adjusted / size.width
}

/// An indeterminate linear indicator, as Compose's `LinearProgressIndicator()` (Material 3 1.3
/// on): two lines sweeping across over 1.75 s, accelerating, the track between and around them
/// past `gap_size`, forever.
#[composable]
pub fn IndeterminateLinearProgressIndicator(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::ProgressIndicatorDefaults::linear_color())] color: Color,
    #[default(crate::ProgressIndicatorDefaults::linear_track_color())] track_color: Color,
    #[default(crate::ProgressIndicatorDefaults::LINEAR_STROKE_CAP)] stroke_cap: StrokeCap,
    #[default(crate::ProgressIndicatorDefaults::LINEAR_INDICATOR_TRACK_GAP_SIZE)] gap_size: Dp,
) {
    let transition = remember_infinite_transition("LinearProgressIndicator");
    let time: State<f32> = transition.animate_float(
        0.0,
        LINEAR_DURATION as f32,
        infinite_repeatable(tween(LINEAR_DURATION).easing(Easing::Linear), RepeatMode::Restart),
    );
    // Its own repaint boundary, as the circular one's.
    let modifier = increase_semantics_bounds(modifier, false);
    let modifier = linear_size(progress_semantics(modifier, None)).repaint_boundary();
    compose_foundation::layout::Canvas(modifier, move |scope| {
        let gap = linear_gap_fraction(scope, gap_size, stroke_cap);
        let time = time.get();
        let (head1, tail1) = (FIRST_HEAD.at(time), FIRST_TAIL.at(time));
        let (head2, tail2) = (SECOND_HEAD.at(time), SECOND_TAIL.at(time));
        // As Compose's: the track before the first line, the first line, the track between
        // the lines, the second line, the track after it.
        if head1 < 1.0 - gap {
            let start = if head1 > 0.0 { head1 + gap } else { 0.0 };
            draw_line_fraction(scope, start, 1.0, track_color, stroke_cap);
        }
        if head1 - tail1 > 0.0 {
            draw_line_fraction(scope, head1, tail1, color, stroke_cap);
        }
        if tail1 > gap {
            let start = if head2 > 0.0 { head2 + gap } else { 0.0 };
            let end = if tail1 < 1.0 { tail1 - gap } else { 1.0 };
            draw_line_fraction(scope, start, end, track_color, stroke_cap);
        }
        if head2 - tail2 > 0.0 {
            draw_line_fraction(scope, head2, tail2, color, stroke_cap);
        }
        if tail2 > gap {
            let end = if tail2 < 1.0 { tail2 - gap } else { 1.0 };
            draw_line_fraction(scope, 0.0, end, track_color, stroke_cap);
        }
    });
}

/// The indeterminate linear cycle, as `LinearAnimationDuration`.
const LINEAR_DURATION: u32 = 1750;

/// One end of a line of the indeterminate linear indicator: from 0 to 1 over `duration` ms
/// after `delay`, eased, as Compose's keyframes.
struct LineEnd {
    delay: f32,
    duration: f32,
}

/// The lines' ends' easing, as `LinearIndeterminateProgressEasing`
/// (`MotionTokens.EasingEmphasizedAccelerateCubicBezier`).
const LINEAR_EASING: Easing = Easing::cubic_bezier(0.3, 0.0, 0.8, 0.15);

impl LineEnd {
    fn at(&self, time: f32) -> f32 {
        LINEAR_EASING.transform(((time - self.delay) / self.duration).clamp(0.0, 1.0))
    }
}

const FIRST_HEAD: LineEnd = LineEnd { delay: 0.0, duration: 1000.0 };
const FIRST_TAIL: LineEnd = LineEnd { delay: 250.0, duration: 1000.0 };
const SECOND_HEAD: LineEnd = LineEnd { delay: 650.0, duration: 850.0 };
const SECOND_TAIL: LineEnd = LineEnd { delay: 900.0, duration: 850.0 };

/// 240 by 4 dp unless the modifier says otherwise, as Compose's.
fn linear_size(modifier: Modifier) -> Modifier {
    modifier.size((
        ProgressIndicatorDefaults::LINEAR_INDICATOR_WIDTH,
        ProgressIndicatorDefaults::LINEAR_INDICATOR_HEIGHT,
    ))
}

/// A line across the node's middle from `start` to `end` (fractions of its width, from its
/// start; either way round), as thick as the node, as Compose's `drawLinearIndicator`: butt
/// ends when the node is taller than wide (no room for the caps), else the caps kept inside
/// it, and none drawn if it's empty.
fn draw_line_fraction(
    scope: &mut DrawScope<'_>,
    start: f32,
    end: f32,
    color: Color,
    cap: StrokeCap,
) {
    let size = scope.size();
    let stroke = size.height;
    let rtl = scope.layout_direction() == compose_ui::LayoutDirection::Rtl;
    let (mut from, mut to) = if rtl {
        ((1.0 - end) * size.width, (1.0 - start) * size.width)
    } else {
        (start * size.width, end * size.width)
    };
    let y = size.height / 2.0;
    if cap == StrokeCap::Butt || stroke > size.width {
        scope.draw_line(
            Offset::new(from, y),
            Offset::new(to, y),
            Paint::stroke_with(color, stroke, StrokeCap::Butt),
        );
        return;
    }
    if (end - start).abs() == 0.0 {
        return;
    }
    let (low, high) = (stroke / 2.0, size.width - stroke / 2.0);
    from = from.clamp(low, high);
    to = to.clamp(low, high);
    scope.draw_line(
        Offset::new(from, y),
        Offset::new(to, y),
        Paint::stroke_with(color, stroke, cap),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_indeterminate_arc_grows_then_shrinks_as_it_turns() {
        let (rotation, progress) = circular_indeterminate_state(0.0);
        assert_eq!((rotation, progress), (0.0, CIRCULAR_MIN_PROGRESS));
        // Half way: the longest arc; the global turn at 540°, three quarter-turn jumps done
        // (and a fourth not started): 540 + 180 (the jump at 3 s not started) = 720.
        let (rotation, progress) = circular_indeterminate_state(3000.0);
        assert!((progress - CIRCULAR_MAX_PROGRESS).abs() < 1e-6);
        assert!((rotation - (540.0 + 180.0) % 360.0).abs() < 1e-3, "{rotation}");
        // A jump: a quarter turn in its first 300 ms.
        let (before, _) = circular_indeterminate_state(1500.0);
        let (after, _) = circular_indeterminate_state(1800.0);
        assert!((after - before - (90.0 + 300.0 / 6000.0 * 1080.0)).abs() < 1e-3);
        let (_, end) = circular_indeterminate_state(6000.0);
        assert!((end - CIRCULAR_MIN_PROGRESS).abs() < 1e-6);
    }
}
