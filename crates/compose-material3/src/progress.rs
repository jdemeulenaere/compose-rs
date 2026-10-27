//! `CircularProgressIndicator` and `LinearProgressIndicator`, as Compose's Material 3:
//! determinate (a progress from 0 to 1) or indeterminate (animated forever), their animations
//! read while drawing so each frame only draws again.

use compose_animation::{
    Easing, RepeatMode, infinite_repeatable, remember_infinite_transition, tween,
};
use compose_runtime::{State, composable};
use compose_ui::{Color, Dp, Modifier, Offset, Paint, ProgressBarRangeInfo, Rect, StrokeCap};

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
    /// Between a determinate linear indicator's line and its track, as
    /// `LinearIndicatorTrackGapSize` (`LinearProgressIndicatorTokens.TrackActiveSpace`).
    pub const LINEAR_INDICATOR_TRACK_GAP_SIZE: Dp = Dp(4.0);
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

    /// The indicator's color, as `circularColor` and `linearColor`.
    pub fn color() -> Color {
        MaterialTheme::color_scheme().primary
    }

    /// A linear indicator's track, as `linearTrackColor`: the secondary container
    /// (`ProgressIndicatorTokens.TrackColor`, Material 3 1.3 on; the surface variant before).
    pub fn linear_track_color() -> Color {
        MaterialTheme::color_scheme().secondary_container
    }
}

/// How the progress is read: a value (determinate) or none (indeterminate).
fn progress_semantics(modifier: Modifier, progress: Option<f32>) -> Modifier {
    modifier.semantics_with(true, move |config| {
        config.set_progress_bar_range_info(ProgressBarRangeInfo {
            current: progress.map_or(0.0, |progress| progress.clamp(0.0, 1.0)),
            range: if progress.is_some() { (0.0, 1.0) } else { (0.0, 0.0) },
            steps: 0,
        });
    })
}

/// A determinate circular indicator, as Compose's `CircularProgressIndicator(progress, …)`: an
/// arc from the top, clockwise, `progress` (0 to 1) of the way round, over its track.
#[composable]
pub fn CircularProgressIndicator(
    progress: f32,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::ProgressIndicatorDefaults::color())] color: Color,
    #[default(crate::ProgressIndicatorDefaults::CIRCULAR_STROKE_WIDTH)] stroke_width: Dp,
    #[default(crate::Color::TRANSPARENT)] track_color: Color,
    #[default(crate::ProgressIndicatorDefaults::CIRCULAR_DETERMINATE_STROKE_CAP)]
    stroke_cap: StrokeCap,
) {
    let modifier = progress_semantics(modifier, Some(progress))
        .size(ProgressIndicatorDefaults::CIRCULAR_INDICATOR_DIAMETER);
    compose_foundation::layout::Canvas(modifier, move |scope| {
        let stroke = scope.density().to_px(stroke_width);
        let oval = inset_oval(scope.size().width.min(scope.size().height), stroke);
        if track_color.alpha() > 0 {
            scope.draw_arc(oval, 0.0, 360.0, Paint::stroke_with(track_color, stroke, stroke_cap));
        }
        let sweep = progress.clamp(0.0, 1.0) * 360.0;
        scope.draw_arc(oval, -90.0, sweep, Paint::stroke_with(color, stroke, stroke_cap));
    });
}

/// An indeterminate circular indicator, as Compose's `CircularProgressIndicator()`: an arc
/// growing and shrinking as it turns, forever.
#[composable]
pub fn IndeterminateCircularProgressIndicator(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::ProgressIndicatorDefaults::color())] color: Color,
    #[default(crate::ProgressIndicatorDefaults::CIRCULAR_STROKE_WIDTH)] stroke_width: Dp,
    #[default(crate::Color::TRANSPARENT)] track_color: Color,
    #[default(crate::ProgressIndicatorDefaults::CIRCULAR_INDETERMINATE_STROKE_CAP)]
    stroke_cap: StrokeCap,
) {
    // One clock for the five turns of a cycle (Compose's `RotationsPerCycle`), the angles from
    // it as Compose's keyframes.
    let transition = remember_infinite_transition("CircularProgressIndicator");
    let cycle = (ROTATION_DURATION * ROTATIONS_PER_CYCLE) as f32;
    let time: State<f32> = transition.animate_float(
        0.0,
        cycle,
        infinite_repeatable(
            tween(ROTATION_DURATION * ROTATIONS_PER_CYCLE).easing(Easing::Linear),
            RepeatMode::Restart,
        ),
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
        if track_color.alpha() > 0 {
            scope.draw_arc(oval, 0.0, 360.0, Paint::stroke_with(track_color, stroke, stroke_cap));
        }
        let (start, sweep) = circular_indeterminate_angles(time.get());
        // A cap past the arc's ends: the arc starts that much later, as Compose's.
        let cap_offset = if stroke_cap == StrokeCap::Butt || size == 0.0 {
            0.0
        } else {
            (stroke / (size / 2.0)).to_degrees() / 2.0
        };
        scope.draw_arc(
            oval,
            start + cap_offset,
            sweep.max(0.1),
            Paint::stroke_with(color, stroke, stroke_cap),
        );
    });
}

const ROTATION_DURATION: u32 = 1332;
const ROTATIONS_PER_CYCLE: u32 = 5;
const BASE_ROTATION_ANGLE: f32 = 286.0;
const JUMP_ROTATION_ANGLE: f32 = 290.0;
const ROTATION_ANGLE_OFFSET: f32 = (BASE_ROTATION_ANGLE + JUMP_ROTATION_ANGLE) % 360.0;
const CIRCULAR_EASING: Easing = Easing::cubic_bezier(0.4, 0.0, 0.2, 1.0);

/// The indeterminate arc's start and sweep, in degrees, `time` milliseconds into a cycle, as
/// Compose's `CircularProgressIndicator` keyframes.
fn circular_indeterminate_angles(time: f32) -> (f32, f32) {
    let rotation = (time / ROTATION_DURATION as f32).floor();
    let local = time % ROTATION_DURATION as f32;
    let half = ROTATION_DURATION as f32 / 2.0;
    let base_rotation = local / ROTATION_DURATION as f32 * BASE_ROTATION_ANGLE;
    // The head jumps ahead in the first half, the tail catches up in the second.
    let end = JUMP_ROTATION_ANGLE * CIRCULAR_EASING.transform((local / half).min(1.0));
    let start = JUMP_ROTATION_ANGLE * CIRCULAR_EASING.transform(((local - half) / half).max(0.0));
    let offset = -90.0 + (rotation * ROTATION_ANGLE_OFFSET) % 360.0 + base_rotation;
    (start + offset, (end - start).abs())
}

/// The square `size` wide, inset by half the stroke, its arc drawn in.
fn inset_oval(size: f32, stroke: f32) -> Rect {
    let half = stroke / 2.0;
    Rect::new(half, half, size - half, size - half)
}

/// A determinate linear indicator, as Compose's `LinearProgressIndicator(progress, …)` (Material
/// 3 1.4): a line from the start, `progress` (0 to 1) of the way, then `gap_size` and the track
/// to the end; 240 by 4 dp. A capped line's gap grows by the stroke, so its caps don't cross
/// into it (a gap of 0: the line's and the track's round ends touch). Compose also draws a stop
/// dot at the track's end (`drawStopIndicator`), which this doesn't (D572).
#[composable]
pub fn LinearProgressIndicator(
    progress: f32,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::ProgressIndicatorDefaults::color())] color: Color,
    #[default(crate::ProgressIndicatorDefaults::linear_track_color())] track_color: Color,
    #[default(crate::ProgressIndicatorDefaults::LINEAR_STROKE_CAP)] stroke_cap: StrokeCap,
    #[default(crate::ProgressIndicatorDefaults::LINEAR_INDICATOR_TRACK_GAP_SIZE)] gap_size: Dp,
) {
    let modifier = linear_size(progress_semantics(modifier, Some(progress)));
    compose_foundation::layout::Canvas(modifier, move |scope| {
        let rtl = scope.layout_direction() == compose_ui::LayoutDirection::Rtl;
        let progress = progress.clamp(0.0, 1.0);
        let track_start = progress + progress.min(linear_gap_fraction(scope, gap_size, stroke_cap));
        if track_start <= 1.0 {
            draw_line_fraction(scope, track_start, 1.0, track_color, stroke_cap, rtl);
        }
        draw_line_fraction(scope, 0.0, progress, color, stroke_cap, rtl);
    });
}

/// The gap between a linear indicator's line and its track, as a fraction of its width:
/// `gap_size`, and the stroke's width for a capped line (its caps reach half a stroke past each
/// end), as Compose's `adjustedGapSize`.
fn linear_gap_fraction(scope: &compose_ui::DrawScope<'_>, gap_size: Dp, cap: StrokeCap) -> f32 {
    let size = scope.size();
    if size.width <= 0.0 {
        return 0.0;
    }
    let gap = scope.density().to_px(gap_size);
    let adjusted =
        if cap == StrokeCap::Butt || size.height > size.width { gap } else { gap + size.height };
    adjusted / size.width
}

/// An indeterminate linear indicator, as Compose's `LinearProgressIndicator()`: two lines
/// sweeping across the track, forever.
#[composable]
pub fn IndeterminateLinearProgressIndicator(
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(crate::ProgressIndicatorDefaults::color())] color: Color,
    #[default(crate::ProgressIndicatorDefaults::linear_track_color())] track_color: Color,
    #[default(crate::ProgressIndicatorDefaults::LINEAR_STROKE_CAP)] stroke_cap: StrokeCap,
) {
    let transition = remember_infinite_transition("LinearProgressIndicator");
    let time: State<f32> = transition.animate_float(
        0.0,
        LINEAR_DURATION as f32,
        infinite_repeatable(tween(LINEAR_DURATION).easing(Easing::Linear), RepeatMode::Restart),
    );
    // Its own repaint boundary, as the circular one's.
    let modifier = linear_size(progress_semantics(modifier, None)).repaint_boundary();
    compose_foundation::layout::Canvas(modifier, move |scope| {
        let rtl = scope.layout_direction() == compose_ui::LayoutDirection::Rtl;
        draw_line_fraction(scope, 0.0, 1.0, track_color, stroke_cap, rtl);
        let time = time.get();
        for (head, tail) in [(FIRST_HEAD, FIRST_TAIL), (SECOND_HEAD, SECOND_TAIL)] {
            let (head, tail) = (head.at(time), tail.at(time));
            if head - tail > 0.0 {
                draw_line_fraction(scope, tail, head, color, stroke_cap, rtl);
            }
        }
    });
}

const LINEAR_DURATION: u32 = 1800;

/// One end of a line of the indeterminate linear indicator: from 0 to 1 over `duration` ms
/// after `delay`, eased, as Compose's keyframes.
struct LineEnd {
    delay: f32,
    duration: f32,
    easing: Easing,
}

impl LineEnd {
    fn at(&self, time: f32) -> f32 {
        self.easing.transform(((time - self.delay) / self.duration).clamp(0.0, 1.0))
    }
}

const FIRST_HEAD: LineEnd =
    LineEnd { delay: 0.0, duration: 750.0, easing: Easing::cubic_bezier(0.2, 0.0, 0.8, 1.0) };
const FIRST_TAIL: LineEnd =
    LineEnd { delay: 333.0, duration: 850.0, easing: Easing::cubic_bezier(0.4, 0.0, 1.0, 1.0) };
const SECOND_HEAD: LineEnd =
    LineEnd { delay: 1000.0, duration: 567.0, easing: Easing::cubic_bezier(0.0, 0.0, 0.65, 1.0) };
const SECOND_TAIL: LineEnd =
    LineEnd { delay: 1267.0, duration: 533.0, easing: Easing::cubic_bezier(0.1, 0.0, 0.45, 1.0) };

/// 240 by 4 dp unless the modifier says otherwise, as Compose's.
fn linear_size(modifier: Modifier) -> Modifier {
    modifier.size((
        ProgressIndicatorDefaults::LINEAR_INDICATOR_WIDTH,
        ProgressIndicatorDefaults::LINEAR_INDICATOR_HEIGHT,
    ))
}

/// A line across the node's middle from `start` to `end` (fractions of its width, from its
/// start), as thick as the node; a capped line's caps kept inside it, and none drawn if it's
/// empty, as Compose's `drawLinearIndicator`.
fn draw_line_fraction(
    scope: &mut compose_ui::DrawScope<'_>,
    start: f32,
    end: f32,
    color: Color,
    cap: StrokeCap,
    rtl: bool,
) {
    let size = scope.size();
    let stroke = size.height;
    let (mut from, mut to) = (start * size.width, end * size.width);
    if cap != StrokeCap::Butt && stroke <= size.width {
        if (to - from).abs() == 0.0 {
            return;
        }
        let (low, high) = (stroke / 2.0, size.width - stroke / 2.0);
        from = from.clamp(low, high);
        to = to.clamp(low, high);
    }
    let (from, to) = if rtl { (size.width - from, size.width - to) } else { (from, to) };
    let y = size.height / 2.0;
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
    fn the_indeterminate_arc_starts_at_the_top_and_grows_then_shrinks() {
        assert_eq!(circular_indeterminate_angles(0.0), (-90.0, 0.0));
        let (_, growing) = circular_indeterminate_angles(333.0);
        let (_, longest) = circular_indeterminate_angles(666.0);
        let (_, shrinking) = circular_indeterminate_angles(1000.0);
        assert!(growing > 0.0 && growing < longest && shrinking < longest);
        assert_eq!(longest, JUMP_ROTATION_ANGLE);
    }
}
