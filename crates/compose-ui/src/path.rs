//! Paths, as Compose's `Path`: lines, curves, shapes and closes, filled or stroked, built in code
//! or parsed from SVG path data (`PathParser`), which is how vector icons ship.
//!
//! A [`Path`] is mutable and shared, as Compose's (D530): remembered once and reset in a drawing
//! block each frame, as Compose code does. Drawing it records its contents as they are then, a
//! [`PathData`] that never changes once shared: a recording keeps what it drew, as a render node
//! keeps its copy of an `android.graphics.Path` (D531). Each change gives the path a new
//! generation, so a platform keeping a path by identity (Android's replayer) redefines it in
//! place (D532).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::unit::{Offset, Rect};

/// One drawing command of a path.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PathCommand {
    MoveTo(Offset),
    LineTo(Offset),
    /// A quadratic Bézier curve: the control point, the end.
    QuadTo(Offset, Offset),
    /// A cubic Bézier curve: two control points, the end.
    CubicTo(Offset, Offset, Offset),
    Close,
}

/// Which points are inside a path that crosses itself, as `PathFillType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PathFillType {
    #[default]
    NonZero,
    EvenOdd,
}

/// The direction a closed shape added to a path winds, as `Path.Direction`: with
/// [`PathFillType::NonZero`], shapes winding opposite ways cut holes in each other.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PathDirection {
    #[default]
    CounterClockwise,
    Clockwise,
}

/// How [`Path::op`] combines two paths, as `PathOperation`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PathOperation {
    /// The first path, less the second.
    Difference,
    /// Where both paths are.
    Intersect,
    /// Where either path is.
    Union,
    /// Where exactly one of the paths is.
    Xor,
    /// The second path, less the first.
    ReverseDifference,
}

/// A rectangle with rounded corners, as Compose's `RoundRect`: each corner's radii.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RoundRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub top_left_corner_radius: CornerRadius,
    pub top_right_corner_radius: CornerRadius,
    pub bottom_right_corner_radius: CornerRadius,
    pub bottom_left_corner_radius: CornerRadius,
}

impl RoundRect {
    /// `rect` with every corner rounded by `corner_radius`, as `RoundRect(rect, cornerRadius)`.
    pub fn new(rect: Rect, corner_radius: CornerRadius) -> RoundRect {
        RoundRect {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
            top_left_corner_radius: corner_radius,
            top_right_corner_radius: corner_radius,
            bottom_right_corner_radius: corner_radius,
            bottom_left_corner_radius: corner_radius,
        }
    }

    pub fn rect(&self) -> Rect {
        Rect::new(self.left, self.top, self.right, self.bottom)
    }
}

/// A corner's radii, horizontal and vertical, as Compose's `CornerRadius`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct CornerRadius {
    pub x: f32,
    pub y: f32,
}

impl CornerRadius {
    pub const ZERO: CornerRadius = CornerRadius { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> CornerRadius {
        CornerRadius { x, y }
    }
}

/// A transform of 2D points as Compose's `Matrix`: 4 × 4 values, column-major (`values[col * 4 +
/// row]`), as Compose's `Matrix.values`; a point (x, y) maps to (x', y') through the first,
/// second and fourth rows, divided by w (the fourth) when it isn't 1.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Matrix {
    pub values: [f32; 16],
}

impl Default for Matrix {
    fn default() -> Matrix {
        Matrix::new()
    }
}

impl Matrix {
    /// The identity, as `Matrix()`.
    pub const fn new() -> Matrix {
        let mut values = [0.0; 16];
        values[0] = 1.0;
        values[5] = 1.0;
        values[10] = 1.0;
        values[15] = 1.0;
        Matrix { values }
    }

    /// A move by (`x`, `y`).
    pub fn translation(x: f32, y: f32) -> Matrix {
        let mut matrix = Matrix::new();
        matrix.values[12] = x;
        matrix.values[13] = y;
        matrix
    }

    /// A scale by `x` and `y` around the origin.
    pub fn scaling(x: f32, y: f32) -> Matrix {
        let mut matrix = Matrix::new();
        matrix.values[0] = x;
        matrix.values[5] = y;
        matrix
    }

    /// A rotation by `degrees` (clockwise, y down) around the origin, as `rotateZ`.
    pub fn rotation(degrees: f32) -> Matrix {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let mut matrix = Matrix::new();
        matrix.values[0] = cos;
        matrix.values[1] = sin;
        matrix.values[4] = -sin;
        matrix.values[5] = cos;
        matrix
    }

    /// Where `point` goes, as `map(Offset)`.
    pub fn map(&self, point: Offset) -> Offset {
        let v = &self.values;
        let x = v[0] * point.x + v[4] * point.y + v[12];
        let y = v[1] * point.x + v[5] * point.y + v[13];
        let w = v[3] * point.x + v[7] * point.y + v[15];
        // As Compose's: a point mapped to infinity (w = 0) is the origin.
        let scale = 1.0 / w;
        let scale = if scale.is_finite() { scale } else { 0.0 };
        Offset::new(x * scale, y * scale)
    }
}

/// Each path's identity: a path keeps it while it changes (Android's replayer keeps one
/// `android.graphics.Path` per identity and generation it was drawn at). 0 is the operands'
/// contents of a path's steps, which are no path's.
static NEXT_IDENTITY: AtomicU64 = AtomicU64::new(1);

/// Computes [`Path::op`] when it's called: the platform's path operations (desktop: Skia's,
/// installed by compose-skia), given the operation and both operands' contents, returning the
/// result's commands and fill type, or `None` if it fails. Without one (Android, whose path
/// operations are in its replayer, and headless tests), a path keeps the operation as steps
/// that each platform computes when it draws them (D535).
pub type PathOperator =
    fn(PathOperation, &PathData, &PathData) -> Option<(Vec<PathCommand>, PathFillType)>;

thread_local! {
    /// The thread's [`PathOperator`]: paths belong to their thread, so does how they combine.
    static PATH_OPERATOR: std::cell::Cell<Option<PathOperator>> = const { std::cell::Cell::new(None) };
}

/// Sets the platform's path operations ([`PathOperator`]) for this thread's paths (`None`: kept as
/// steps): the desktop application sets Skia's on its thread; a test sets them where it wants
/// them, so other tests aren't affected.
pub fn set_path_operator(operator: Option<PathOperator>) {
    PATH_OPERATOR.set(operator);
}

/// One step of the operations a path's contents start with (D535): a stack machine whose
/// steps leave one path, to which the contents' commands are added, then their fill type set.
/// As Compose's `Path.op` on Android leaves to `android.graphics.Path.op`, these are computed by
/// the platform drawing them, where no [`PathOperator`] computed them when they were made.
#[derive(Debug, PartialEq)]
pub enum PathStep {
    /// Pushes contents that have no steps of their own.
    Push(Arc<PathData>),
    /// Pops the top two paths and pushes the first (below) combined with the second by the
    /// operation, as `Path.op(path1, path2, operation)`; the first itself if the platform can't
    /// combine them (as Android's `Path.op` leaves the path it's called on).
    Combine(PathOperation),
    /// Pops the top three paths and pushes the second combined with the third (the top) by the
    /// operation, or the first (the path's contents before `op`) if the platform can't combine
    /// them, as Android's `path.op(path1, path2, operation)` leaves `path` as it was.
    CombineOr(PathOperation),
    /// Pops the top path and adds its contours to the one below, as `addPath`.
    Add,
    /// Sets the top path's fill type.
    SetFillType(PathFillType),
}

/// What a path holds at one generation: its steps (operations, D535), commands and fill type.
/// Shared (`Arc`) by what drew it, it never changes: a [`Path`] changed after drawing gets new
/// contents, with the same [`id`](PathData::id) and a new [`generation`](PathData::generation).
/// Not `Clone`: contents are known by their identity and generation, which a copy would share.
///
/// ```compile_fail
/// let path = compose_ui::Path::new();
/// let copy: compose_ui::PathData = (*path.data()).clone();
/// ```
#[derive(Debug)]
pub struct PathData {
    id: u64,
    generation: u64,
    fill_type: PathFillType,
    steps: Vec<PathStep>,
    commands: Vec<PathCommand>,
    /// Where the next relative command starts: the last command's end.
    current: Offset,
    /// The last contour's start, where a close leaves the current point.
    contour_start: Offset,
    /// A command after this one draws from `current`: `false` until the first command (an
    /// `arc_to` then moves rather than draws a line).
    has_current: bool,
    /// Its steps' bounds once computed (`steps_bounds`): contents with steps never change once
    /// shared, so a frame asking again (`get_bounds`, `is_empty`) doesn't run them again.
    steps_bounds: std::sync::OnceLock<Option<Rect>>,
}

/// Contents only: two paths' contents are equal whatever their identities and generations.
impl PartialEq for PathData {
    fn eq(&self, other: &Self) -> bool {
        self.fill_type == other.fill_type
            && self.commands == other.commands
            && self.steps == other.steps
    }
}

impl PathData {
    fn empty() -> PathData {
        PathData::with_id(NEXT_IDENTITY.fetch_add(1, Ordering::Relaxed))
    }

    fn with_id(id: u64) -> PathData {
        PathData {
            id,
            generation: 0,
            fill_type: PathFillType::NonZero,
            steps: Vec::new(),
            commands: Vec::new(),
            current: Offset::ZERO,
            contour_start: Offset::ZERO,
            has_current: false,
            steps_bounds: std::sync::OnceLock::new(),
        }
    }

    /// Contents of no path (identity 0), holding `commands`: an operand's.
    fn operand(commands: Vec<PathCommand>, fill_type: PathFillType) -> PathData {
        let mut data = PathData::with_id(0);
        data.fill_type = fill_type;
        for command in commands {
            data.push(command);
        }
        data
    }

    /// A copy under the same identity, a generation further: the start of a change.
    fn next_generation(&self) -> PathData {
        PathData {
            id: self.id,
            generation: self.generation + 1,
            fill_type: self.fill_type,
            steps: self
                .steps
                .iter()
                .map(|step| match step {
                    PathStep::Push(data) => PathStep::Push(Arc::clone(data)),
                    PathStep::Combine(operation) => PathStep::Combine(*operation),
                    PathStep::CombineOr(operation) => PathStep::CombineOr(*operation),
                    PathStep::Add => PathStep::Add,
                    PathStep::SetFillType(fill_type) => PathStep::SetFillType(*fill_type),
                })
                .collect(),
            commands: self.commands.clone(),
            current: self.current,
            contour_start: self.contour_start,
            has_current: self.has_current,
            steps_bounds: std::sync::OnceLock::new(),
        }
    }

    /// The identity of the path these contents are of.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Which of its path's contents these are: a path's generation changes with each change.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn fill_type(&self) -> PathFillType {
        self.fill_type
    }

    /// The operations' steps its contents start with (empty: none), D535.
    pub fn steps(&self) -> &[PathStep] {
        &self.steps
    }

    /// The commands, added after its steps' result if it has steps.
    pub fn commands(&self) -> &[PathCommand] {
        &self.commands
    }

    /// Runs its steps (iteratively, however many: an operand's contents have none) with
    /// `push` making a value of an operand's contents, `combine` combining two (the first if it
    /// can't), `combine_or` combining the second and third or else giving the first, `add` adding
    /// the second to the first and `fill` setting a fill type; `None` without steps.
    pub fn evaluate_steps<T>(
        &self,
        mut push: impl FnMut(&PathData) -> T,
        mut combine: impl FnMut(T, T, PathOperation) -> T,
        mut combine_or: impl FnMut(T, T, T, PathOperation) -> T,
        mut add: impl FnMut(T, T) -> T,
        mut fill: impl FnMut(&mut T, PathFillType),
    ) -> Option<T> {
        let mut stack: Vec<T> = Vec::new();
        for step in &self.steps {
            match step {
                PathStep::Push(data) => stack.push(push(data)),
                PathStep::Combine(operation) => {
                    let second = stack.pop()?;
                    let first = stack.pop()?;
                    stack.push(combine(first, second, *operation));
                }
                PathStep::CombineOr(operation) => {
                    let second = stack.pop()?;
                    let first = stack.pop()?;
                    let fallback = stack.pop()?;
                    stack.push(combine_or(fallback, first, second, *operation));
                }
                PathStep::Add => {
                    let second = stack.pop()?;
                    let first = stack.pop()?;
                    stack.push(add(first, second));
                }
                PathStep::SetFillType(fill_type) => fill(stack.last_mut()?, *fill_type),
            }
        }
        stack.pop()
    }

    /// Whether it draws nothing, as `isEmpty`: no command, and steps whose result is known to
    /// be empty (a union of empty paths, an intersection of paths whose bounds don't meet, a
    /// difference from an empty path). A result empty for another reason (a path less itself)
    /// isn't known before a platform computes it: where Skia computes operations when they're
    /// made (desktop), that's exact.
    pub fn is_empty(&self) -> bool {
        if !self.commands.is_empty() {
            return false;
        }
        self.steps_bounds().is_none()
    }

    /// The bounds of its steps' result as their operands' bounds tell (`None`: empty, or no
    /// steps).
    fn steps_bounds(&self) -> Option<Rect> {
        if self.steps.is_empty() {
            return None;
        }
        *self.steps_bounds.get_or_init(|| {
            let bounds = |data: &PathData| commands_bounds(&data.commands);
            // A combination that fails gives the path's earlier contents: bounded by either.
            let combine_or = |fallback, first, second, operation| {
                union(fallback, combined_bounds(first, second, operation))
            };
            self.evaluate_steps(bounds, combined_bounds, combine_or, union, |_, _| {}).flatten()
        })
    }

    /// The bounds of its points, curves' control points included, as `getBounds` (Skia's and
    /// Android's: not the curves' tight bounds). Steps not computed yet (D535) are bounded by
    /// their operands' bounds as the operations combine them (the first's for a difference, the
    /// overlap for an intersection), which may be larger than the result's. Empty: all zeros.
    pub fn bounds(&self) -> Rect {
        union(self.steps_bounds(), commands_bounds(&self.commands))
            .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0))
    }

    /// Whether it's one convex contour, as `isConvex`: its points (curves' control points
    /// included) turn one way. Contents with steps aren't known to be (false), as Android's
    /// before its path computes.
    pub fn is_convex(&self) -> bool {
        if !self.steps.is_empty() {
            return false;
        }
        let mut points: Vec<Offset> = Vec::new();
        for (index, command) in self.commands.iter().enumerate() {
            match command {
                PathCommand::MoveTo(point) => {
                    // A second contour (a move after drawing) isn't one convex shape.
                    if index > 0 && points.len() > 1 {
                        return false;
                    }
                    points.clear();
                    points.push(*point);
                }
                PathCommand::LineTo(point) => points.push(*point),
                PathCommand::QuadTo(control, end) => points.extend([*control, *end]),
                PathCommand::CubicTo(first, second, end) => points.extend([*first, *second, *end]),
                PathCommand::Close => {}
            }
        }
        points.dedup();
        if points.len() > 1 && points.first() == points.last() {
            points.pop();
        }
        // Fewer than 3 points (none, a point, a line) are convex, as Skia's.
        if points.len() < 3 {
            return true;
        }
        let (mut sign, mut turned) = (0.0f32, 0.0f32);
        let count = points.len();
        for index in 0..count {
            let (a, b, c) =
                (points[index], points[(index + 1) % count], points[(index + 2) % count]);
            let cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
            if cross != 0.0 {
                if sign != 0.0 && cross.signum() != sign {
                    return false;
                }
                sign = cross.signum();
            }
            let (first, second) = ((b.y - a.y).atan2(b.x - a.x), (c.y - b.y).atan2(c.x - b.x));
            let mut turn = second - first;
            while turn > std::f32::consts::PI {
                turn -= std::f32::consts::TAU;
            }
            while turn < -std::f32::consts::PI {
                turn += std::f32::consts::TAU;
            }
            turned += turn;
        }
        // Once round, not a star turning the same way twice.
        turned.abs() < 1.5 * std::f32::consts::TAU
    }

    fn push(&mut self, command: PathCommand) {
        let draws = !matches!(command, PathCommand::MoveTo(_) | PathCommand::Close);
        let first = !self.has_current && self.steps.is_empty();
        if draws && (first || matches!(self.commands.last(), Some(PathCommand::Close))) {
            // As Skia's `injectMoveToIfNeeded`: a contour drawn without a move starts at the
            // origin, one after a close where the closed one did; the move is explicit here, so
            // `add_path`, `translate` and `is_convex` see it as Skia does. (After an operation's
            // steps, it starts where the result's last contour did, which only the platform
            // computing them knows: left to it, D535.)
            let start = if self.has_current { self.contour_start } else { Offset::ZERO };
            self.push(PathCommand::MoveTo(start));
        }
        if matches!(command, PathCommand::Close) && self.commands.is_empty() {
            // Nothing to close, as Skia's.
            return;
        }
        match command {
            PathCommand::MoveTo(point) => {
                self.current = point;
                self.contour_start = point;
            }
            PathCommand::LineTo(end)
            | PathCommand::QuadTo(_, end)
            | PathCommand::CubicTo(_, _, end) => self.current = end,
            PathCommand::Close => self.current = self.contour_start,
        }
        self.has_current = true;
        self.commands.push(command);
    }

    /// A line to `point` unless a contour is open there: what an arc or a shape joining the
    /// path draws first.
    fn line_or_move_to(&mut self, point: Offset, force_move_to: bool) {
        if force_move_to || !self.has_current {
            self.push(PathCommand::MoveTo(point));
        } else if self.current != point {
            self.push(PathCommand::LineTo(point));
        }
    }

    /// An arc as Skia's `arcTo` draws it: a sweep of a whole number of turns (start and end
    /// at the same angle) is a lone point, a move or line to its end, and a larger sweep is its
    /// remainder; else [`arc`](PathData::arc).
    fn arc_to(&mut self, oval: Rect, start: f32, sweep: f32, force_move_to: bool) {
        let turn = std::f32::consts::TAU;
        if sweep.abs() >= turn {
            let rest = sweep % turn;
            // Within Skia's float precision of whole turns.
            if rest.abs() < 1e-5 || (turn - rest.abs()) < 1e-5 {
                let (center_x, center_y) =
                    ((oval.left + oval.right) / 2.0, (oval.top + oval.bottom) / 2.0);
                let end = start + sweep;
                let point = Offset::new(
                    center_x + oval.width() / 2.0 * end.cos(),
                    center_y + oval.height() / 2.0 * end.sin(),
                );
                if force_move_to || !self.has_current {
                    self.push(PathCommand::MoveTo(point));
                } else {
                    self.push(PathCommand::LineTo(point));
                }
                return;
            }
            self.arc(oval, start, rest, force_move_to);
            return;
        }
        self.arc(oval, start, sweep, force_move_to);
    }

    /// The arc of the oval in `oval` from `start` over `sweep` radians (clockwise, y down), as
    /// cubic curves of at most a quarter turn, after a move or line to its start.
    fn arc(&mut self, oval: Rect, start: f32, sweep: f32, force_move_to: bool) {
        let sweep = sweep.clamp(-std::f32::consts::TAU, std::f32::consts::TAU);
        let (center_x, center_y) = ((oval.left + oval.right) / 2.0, (oval.top + oval.bottom) / 2.0);
        let (radius_x, radius_y) = (oval.width() / 2.0, oval.height() / 2.0);
        let point = |angle: f32| {
            Offset::new(center_x + radius_x * angle.cos(), center_y + radius_y * angle.sin())
        };
        self.line_or_move_to(point(start), force_move_to);
        if sweep == 0.0 {
            return;
        }
        let segments = (sweep.abs() / std::f32::consts::FRAC_PI_2).ceil().max(1.0) as usize;
        let step = sweep / segments as f32;
        let handle = 4.0 / 3.0 * (step / 4.0).tan();
        let mut angle = start;
        for _ in 0..segments {
            let next = angle + step;
            let (sin, cos) = angle.sin_cos();
            let (next_sin, next_cos) = next.sin_cos();
            let end = point(next);
            self.push(PathCommand::CubicTo(
                Offset::new(
                    center_x + radius_x * (cos - handle * sin),
                    center_y + radius_y * (sin + handle * cos),
                ),
                Offset::new(
                    center_x + radius_x * (next_cos + handle * next_sin),
                    center_y + radius_y * (next_sin - handle * next_cos),
                ),
                end,
            ));
            angle = next;
        }
    }

    /// Appends steps pushing `data` as an operand: its contents if it has no steps (shared),
    /// else its steps, its commands added, its fill type set.
    fn push_operand(steps: &mut Vec<PathStep>, data: &Arc<PathData>) {
        if data.steps.is_empty() {
            steps.push(PathStep::Push(Arc::clone(data)));
            return;
        }
        steps.extend(data.steps.iter().map(|step| match step {
            PathStep::Push(data) => PathStep::Push(Arc::clone(data)),
            PathStep::Combine(operation) => PathStep::Combine(*operation),
            PathStep::CombineOr(operation) => PathStep::CombineOr(*operation),
            PathStep::Add => PathStep::Add,
            PathStep::SetFillType(fill_type) => PathStep::SetFillType(*fill_type),
        }));
        if !data.commands.is_empty() {
            steps.push(PathStep::Push(Arc::new(PathData::operand(
                data.commands.clone(),
                data.fill_type,
            ))));
            steps.push(PathStep::Add);
        }
        steps.push(PathStep::SetFillType(data.fill_type));
    }

    /// These contents with every point mapped by `map`: their commands, and their operands'.
    fn mapped(&self, map: &impl Fn(Offset) -> Offset) -> PathData {
        let mut data = self.next_generation();
        data.commands = self.commands.iter().map(|command| map_command(command, map)).collect();
        for step in &mut data.steps {
            if let PathStep::Push(operand) = step {
                *step = PathStep::Push(Arc::new(PathData::operand(
                    operand.commands.iter().map(|command| map_command(command, map)).collect(),
                    operand.fill_type,
                )));
            }
        }
        data.current = map(self.current);
        data.contour_start = map(self.contour_start);
        data
    }
}

fn map_command(command: &PathCommand, map: &impl Fn(Offset) -> Offset) -> PathCommand {
    match *command {
        PathCommand::MoveTo(point) => PathCommand::MoveTo(map(point)),
        PathCommand::LineTo(point) => PathCommand::LineTo(map(point)),
        PathCommand::QuadTo(control, end) => PathCommand::QuadTo(map(control), map(end)),
        PathCommand::CubicTo(first, second, end) => {
            PathCommand::CubicTo(map(first), map(second), map(end))
        }
        PathCommand::Close => PathCommand::Close,
    }
}

/// The bounds of `commands`' points (control points included), `None` without any.
fn commands_bounds(commands: &[PathCommand]) -> Option<Rect> {
    let mut bounds: Option<Rect> = None;
    let mut include = |point: &Offset| {
        bounds = union(bounds, Some(Rect::new(point.x, point.y, point.x, point.y)));
    };
    for command in commands {
        match command {
            PathCommand::MoveTo(point) | PathCommand::LineTo(point) => include(point),
            PathCommand::QuadTo(control, end) => {
                include(control);
                include(end);
            }
            PathCommand::CubicTo(first, second, end) => {
                include(first);
                include(second);
                include(end);
            }
            PathCommand::Close => {}
        }
    }
    bounds
}

/// Bounds of `first` combined with `second` by `operation`, from theirs (`None`: empty).
fn combined_bounds(
    first: Option<Rect>,
    second: Option<Rect>,
    operation: PathOperation,
) -> Option<Rect> {
    match operation {
        PathOperation::Difference => first,
        PathOperation::ReverseDifference => second,
        PathOperation::Intersect => match (first, second) {
            (Some(a), Some(b)) => {
                let rect = Rect::new(
                    a.left.max(b.left),
                    a.top.max(b.top),
                    a.right.min(b.right),
                    a.bottom.min(b.bottom),
                );
                (rect.left <= rect.right && rect.top <= rect.bottom).then_some(rect)
            }
            _ => None,
        },
        PathOperation::Union | PathOperation::Xor => union(first, second),
    }
}

fn union(first: Option<Rect>, second: Option<Rect>) -> Option<Rect> {
    match (first, second) {
        (Some(a), Some(b)) => Some(Rect::new(
            a.left.min(b.left),
            a.top.min(b.top),
            a.right.max(b.right),
            a.bottom.max(b.bottom),
        )),
        (rect, None) | (None, rect) => rect,
    }
}

/// A path, as Compose's `Path`: built with `move_to`, `line_to`, shapes and operations, or parsed
/// from SVG path data, and changed in place (`reset`, `rewind`).
///
/// Shared as a Kotlin reference is (D530): a clone is the same path (`Rc`), so a path remembered
/// in composition (`remember(Path::new)`) and captured by a drawing block is the one the block
/// resets and draws, as `remember { Path() }`. It belongs to its thread (`!Send`); its contents
/// ([`data`](Path::data), an `Arc<PathData>`) can go anywhere.
///
/// Drawing it (`DrawScope::draw_path`, `clip_path`) records its contents as they are then, so a
/// path reset and rebuilt between two draws, in the same block or another, draws each shape it
/// had, and a recording keeps what it drew until it's drawn again, as a render node keeps its
/// copy of an `android.graphics.Path` (D531): changing a path draws nothing again, as in Compose.
#[derive(Clone)]
pub struct Path {
    contents: Rc<RefCell<Arc<PathData>>>,
}

impl Default for Path {
    fn default() -> Path {
        Path::new()
    }
}

impl std::fmt::Debug for Path {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("Path").field(&*self.contents.borrow()).finish()
    }
}

/// By identity, as a Kotlin reference: the same path, whatever it holds.
impl compose_runtime::ComposeArg for Path {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.contents, &other.contents)
    }
}

impl Path {
    /// An empty path, filled non-zero, as `Path()`.
    pub fn new() -> Path {
        Path::from_data(PathData::empty())
    }

    fn from_data(data: PathData) -> Path {
        Path { contents: Rc::new(RefCell::new(Arc::new(data))) }
    }

    /// Its contents now, shared: what drawing it records. They never change; the path's later
    /// changes are new contents.
    pub fn data(&self) -> Arc<PathData> {
        Arc::clone(&self.contents.borrow())
    }

    /// Its identity: the same for every clone, and kept as it changes.
    pub fn id(&self) -> u64 {
        self.contents.borrow().id
    }

    /// Its generation: changed by each change.
    pub fn generation(&self) -> u64 {
        self.contents.borrow().generation
    }

    /// Whether it is the same path as `other` (a clone of it), as Kotlin's `===`.
    pub fn ptr_eq(&self, other: &Path) -> bool {
        Rc::ptr_eq(&self.contents, &other.contents)
    }

    /// Changes its contents through `change`, in place when nothing else holds them (nothing
    /// drew them, or what did is gone), else in a copy: what drew them keeps them.
    fn edit<R>(&self, change: impl FnOnce(&mut PathData) -> R) -> R {
        let mut contents = self.contents.borrow_mut();
        match Arc::get_mut(&mut contents) {
            Some(data) => {
                data.generation += 1;
                data.steps_bounds = std::sync::OnceLock::new();
            }
            None => *contents = Arc::new(contents.next_generation()),
        }
        change(Arc::get_mut(&mut contents).expect("unique contents"))
    }

    /// Replaces its contents, a generation further, by `data` (its identity and generation set
    /// here).
    fn replace(&self, mut data: PathData) {
        let mut contents = self.contents.borrow_mut();
        data.id = contents.id;
        data.generation = contents.generation + 1;
        *contents = Arc::new(data);
    }

    /// Empties it, as `edit` changes it, without copying what it held; keeps its fill type if
    /// `keep_fill_type`.
    fn clear(&self, keep_fill_type: bool) {
        let mut contents = self.contents.borrow_mut();
        let fill_type = if keep_fill_type { contents.fill_type } else { PathFillType::NonZero };
        let cleared = |data: &mut PathData| {
            data.fill_type = fill_type;
            data.steps.clear();
            data.commands.clear();
            data.current = Offset::ZERO;
            data.contour_start = Offset::ZERO;
            data.has_current = false;
            data.steps_bounds = std::sync::OnceLock::new();
        };
        match Arc::get_mut(&mut contents) {
            Some(data) => {
                data.generation += 1;
                cleared(data);
            }
            None => {
                let mut data = PathData::with_id(contents.id);
                data.generation = contents.generation + 1;
                // The room the last contents took: a path rebuilt each frame grows once.
                data.commands = Vec::with_capacity(contents.commands.capacity());
                cleared(&mut data);
                *contents = Arc::new(data);
            }
        }
    }

    /// Which points are inside it, as `fillType`.
    pub fn fill_type(&self) -> PathFillType {
        self.contents.borrow().fill_type
    }

    /// Sets which points are inside it, as `fillType = …`.
    pub fn set_fill_type(&self, fill_type: PathFillType) {
        if self.fill_type() != fill_type {
            self.edit(|data| data.fill_type = fill_type);
        }
    }

    /// Whether it draws nothing, as `isEmpty`: see [`PathData::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.contents.borrow().is_empty()
    }

    /// Whether it's one convex contour, as `isConvex`: see [`PathData::is_convex`].
    pub fn is_convex(&self) -> bool {
        self.contents.borrow().is_convex()
    }

    /// The bounds of its points (curves' control points included), as `getBounds`: see
    /// [`PathData::bounds`].
    pub fn get_bounds(&self) -> Rect {
        self.contents.borrow().bounds()
    }

    /// Empties it, keeping its fill type, as `reset` (Android's: its storage is kept here too,
    /// for the next contents).
    pub fn reset(&self) {
        self.clear(true);
    }

    /// Empties it and sets its fill type back to non-zero, keeping its storage for the next
    /// contents, as `rewind` (Skia's `SkPath::rewind`).
    pub fn rewind(&self) {
        self.clear(false);
    }

    /// Starts a contour at (`x`, `y`), as `moveTo`.
    pub fn move_to(&self, x: f32, y: f32) {
        self.edit(|data| data.push(PathCommand::MoveTo(Offset::new(x, y))));
    }

    /// Starts a contour `dx`, `dy` from the current point, as `relativeMoveTo`.
    pub fn relative_move_to(&self, dx: f32, dy: f32) {
        self.edit(|data| data.push(PathCommand::MoveTo(data.current + Offset::new(dx, dy))));
    }

    /// A line to (`x`, `y`), as `lineTo`.
    pub fn line_to(&self, x: f32, y: f32) {
        self.edit(|data| data.push(PathCommand::LineTo(Offset::new(x, y))));
    }

    /// A line `dx`, `dy` from the current point, as `relativeLineTo`.
    pub fn relative_line_to(&self, dx: f32, dy: f32) {
        self.edit(|data| data.push(PathCommand::LineTo(data.current + Offset::new(dx, dy))));
    }

    /// A quadratic Bézier curve through control point (`x1`, `y1`) to (`x2`, `y2`), as
    /// `quadraticTo`.
    pub fn quadratic_to(&self, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.edit(|data| data.push(PathCommand::QuadTo(Offset::new(x1, y1), Offset::new(x2, y2))));
    }

    /// [`quadratic_to`](Path::quadratic_to) with its points from the current point, as
    /// `relativeQuadraticTo`.
    pub fn relative_quadratic_to(&self, dx1: f32, dy1: f32, dx2: f32, dy2: f32) {
        self.edit(|data| {
            let from = data.current;
            data.push(PathCommand::QuadTo(
                from + Offset::new(dx1, dy1),
                from + Offset::new(dx2, dy2),
            ))
        });
    }

    /// A cubic Bézier curve through control points (`x1`, `y1`) and (`x2`, `y2`) to (`x3`, `y3`),
    /// as `cubicTo`.
    pub fn cubic_to(&self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) {
        self.edit(|data| {
            data.push(PathCommand::CubicTo(
                Offset::new(x1, y1),
                Offset::new(x2, y2),
                Offset::new(x3, y3),
            ))
        });
    }

    /// [`cubic_to`](Path::cubic_to) with its points from the current point, as `relativeCubicTo`.
    pub fn relative_cubic_to(&self, dx1: f32, dy1: f32, dx2: f32, dy2: f32, dx3: f32, dy3: f32) {
        self.edit(|data| {
            let from = data.current;
            data.push(PathCommand::CubicTo(
                from + Offset::new(dx1, dy1),
                from + Offset::new(dx2, dy2),
                from + Offset::new(dx3, dy3),
            ))
        });
    }

    /// An arc of the oval in `rect`, from `start_angle_degrees` (0 at 3 o'clock, clockwise) over
    /// `sweep_angle_degrees` (a whole turn at most), as `arcTo`: after a line from the current
    /// point to its start, or a move there if `force_move_to` or nothing is drawn yet.
    pub fn arc_to(
        &self,
        rect: Rect,
        start_angle_degrees: f32,
        sweep_angle_degrees: f32,
        force_move_to: bool,
    ) {
        self.arc_to_rad(
            rect,
            start_angle_degrees.to_radians(),
            sweep_angle_degrees.to_radians(),
            force_move_to,
        );
    }

    /// [`arc_to`](Path::arc_to) in radians, as `arcToRad`.
    pub fn arc_to_rad(
        &self,
        rect: Rect,
        start_angle_radians: f32,
        sweep_angle_radians: f32,
        force_move_to: bool,
    ) {
        self.edit(|data| {
            data.arc_to(rect, start_angle_radians, sweep_angle_radians, force_move_to)
        });
    }

    /// An arc of the oval in `oval` as a contour of its own, as `addArc`: nothing for an empty
    /// oval or no sweep, a whole turn from a quarter's angle (0, 90, …) the closed oval, and
    /// otherwise `arc_to` forced to move (so a whole turn from another angle is a lone point), as
    /// Skia's.
    pub fn add_arc(&self, oval: Rect, start_angle_degrees: f32, sweep_angle_degrees: f32) {
        if oval.width() <= 0.0 || oval.height() <= 0.0 || sweep_angle_degrees == 0.0 {
            return;
        }
        let quarters = start_angle_degrees / 90.0;
        if sweep_angle_degrees.abs() >= 360.0 && (quarters - quarters.round()).abs() < 1e-4 {
            let sweep = std::f32::consts::TAU.copysign(sweep_angle_degrees);
            let start = quarters.round() * std::f32::consts::FRAC_PI_2;
            self.edit(|data| {
                data.arc(oval, start, sweep, true);
                data.push(PathCommand::Close);
            });
            return;
        }
        self.add_arc_rad(oval, start_angle_degrees.to_radians(), sweep_angle_degrees.to_radians());
    }

    /// [`add_arc`](Path::add_arc) in radians, as `addArcRad`.
    pub fn add_arc_rad(&self, oval: Rect, start_angle_radians: f32, sweep_angle_radians: f32) {
        let quarters = start_angle_radians / std::f32::consts::FRAC_PI_2;
        if sweep_angle_radians.abs() >= std::f32::consts::TAU
            && (quarters - quarters.round()).abs() < 1e-4
        {
            self.add_arc(oval, quarters.round() * 90.0, sweep_angle_radians.to_degrees());
            return;
        }
        if oval.width() <= 0.0 || oval.height() <= 0.0 || sweep_angle_radians == 0.0 {
            return;
        }
        self.edit(|data| data.arc_to(oval, start_angle_radians, sweep_angle_radians, true));
    }

    /// `rect` as a closed contour, counterclockwise, as `addRect(rect)`.
    pub fn add_rect(&self, rect: Rect) {
        self.add_rect_with(rect, PathDirection::CounterClockwise);
    }

    /// `rect` as a closed contour winding `direction`, from its top left corner, as
    /// `addRect(rect, direction)`.
    pub fn add_rect_with(&self, rect: Rect, direction: PathDirection) {
        let corners = [
            Offset::new(rect.right, rect.top),
            Offset::new(rect.right, rect.bottom),
            Offset::new(rect.left, rect.bottom),
        ];
        self.edit(|data| {
            data.push(PathCommand::MoveTo(Offset::new(rect.left, rect.top)));
            let mut add = |corner: Offset| data.push(PathCommand::LineTo(corner));
            match direction {
                PathDirection::Clockwise => corners.into_iter().for_each(&mut add),
                PathDirection::CounterClockwise => corners.into_iter().rev().for_each(&mut add),
            }
            data.push(PathCommand::Close);
        });
    }

    /// The oval in `oval` as a closed contour, counterclockwise, as `addOval(oval)`.
    pub fn add_oval(&self, oval: Rect) {
        self.add_oval_with(oval, PathDirection::CounterClockwise);
    }

    /// The oval in `oval` as a closed contour winding `direction`, from its rightmost point, as
    /// `addOval(oval, direction)`.
    pub fn add_oval_with(&self, oval: Rect, direction: PathDirection) {
        let sweep = match direction {
            PathDirection::Clockwise => std::f32::consts::TAU,
            PathDirection::CounterClockwise => -std::f32::consts::TAU,
        };
        self.edit(|data| {
            data.arc(oval, 0.0, sweep, true);
            data.push(PathCommand::Close);
        });
    }

    /// `round_rect` as a closed contour, counterclockwise, as `addRoundRect(roundRect)`.
    pub fn add_round_rect(&self, round_rect: RoundRect) {
        self.add_round_rect_with(round_rect, PathDirection::CounterClockwise);
    }

    /// `round_rect` as a closed contour winding `direction`, as `addRoundRect(roundRect,
    /// direction)`: radii too large for their sides are scaled down together, as Skia's
    /// `SkRRect`, and each corner is a quarter of an ellipse.
    pub fn add_round_rect_with(&self, round_rect: RoundRect, direction: PathDirection) {
        let rect = round_rect.rect();
        let clamp = |radius: CornerRadius| CornerRadius::new(radius.x.max(0.0), radius.y.max(0.0));
        let mut radii = [
            clamp(round_rect.top_left_corner_radius),
            clamp(round_rect.top_right_corner_radius),
            clamp(round_rect.bottom_right_corner_radius),
            clamp(round_rect.bottom_left_corner_radius),
        ];
        let (width, height) = (rect.width().abs(), rect.height().abs());
        let mut scale = 1.0f32;
        for (sum, side) in [
            (radii[0].x + radii[1].x, width),
            (radii[3].x + radii[2].x, width),
            (radii[0].y + radii[3].y, height),
            (radii[1].y + radii[2].y, height),
        ] {
            if sum > side && sum > 0.0 {
                scale = scale.min(side / sum);
            }
        }
        for radius in &mut radii {
            radius.x *= scale;
            radius.y *= scale;
        }
        let [top_left, top_right, bottom_right, bottom_left] = radii;
        let (left, top, right, bottom) = (rect.left, rect.top, rect.right, rect.bottom);
        // Each corner: the rectangle's corner, its oval and the angle its quarter starts at
        // going clockwise.
        let corner = |x: f32, y: f32, center_x: f32, center_y: f32, radius: CornerRadius| {
            let oval = Rect::new(
                center_x - radius.x,
                center_y - radius.y,
                center_x + radius.x,
                center_y + radius.y,
            );
            (Offset::new(x, y), oval, radius.x > 0.0 && radius.y > 0.0)
        };
        let quarter = std::f32::consts::FRAC_PI_2;
        let corners = [
            (corner(right, top, right - top_right.x, top + top_right.y, top_right), -quarter),
            (
                corner(
                    right,
                    bottom,
                    right - bottom_right.x,
                    bottom - bottom_right.y,
                    bottom_right,
                ),
                0.0,
            ),
            (
                corner(left, bottom, left + bottom_left.x, bottom - bottom_left.y, bottom_left),
                quarter,
            ),
            (corner(left, top, left + top_left.x, top + top_left.y, top_left), 2.0 * quarter),
        ];
        self.edit(|data| {
            data.push(PathCommand::MoveTo(Offset::new(left + top_left.x, top)));
            let mut turn = |((point, oval, rounded), start): ((Offset, Rect, bool), f32), sweep| {
                if rounded {
                    data.arc(oval, start, sweep, false);
                } else {
                    data.line_or_move_to(point, false);
                }
            };
            match direction {
                PathDirection::Clockwise => corners.into_iter().for_each(|at| turn(at, quarter)),
                PathDirection::CounterClockwise => corners
                    .into_iter()
                    .rev()
                    .for_each(|(at, start)| turn((at, start + quarter), -quarter)),
            }
            data.push(PathCommand::Close);
        });
    }

    /// Closes the contour with a line to its start, as `close`.
    pub fn close(&self) {
        self.edit(|data| data.push(PathCommand::Close));
    }

    /// Adds `path`'s contours moved by `offset`, as `addPath(path, offset)` (`path` may be this
    /// one: its contents are taken first).
    pub fn add_path(&self, path: &Path, offset: Offset) {
        let other = path.data();
        let other = if offset == Offset::ZERO {
            other
        } else {
            Arc::new(other.mapped(&|point| point + offset))
        };
        if other.steps.is_empty() {
            self.edit(|data| {
                for command in &other.commands {
                    data.push(*command);
                }
            });
            return;
        }
        // Operations to add: this path's contents, then the other's, added (D535).
        let this = self.data();
        let mut steps = Vec::new();
        PathData::push_operand(&mut steps, &this);
        PathData::push_operand(&mut steps, &other);
        steps.push(PathStep::Add);
        let mut data = PathData::with_id(0);
        data.steps = steps;
        data.fill_type = this.fill_type;
        drop(this);
        self.replace(data);
    }

    /// Moves it by `offset`, as `translate`.
    pub fn translate(&self, offset: Offset) {
        if offset != Offset::ZERO {
            let data = self.contents.borrow().mapped(&|point| point + offset);
            self.replace(data);
        }
    }

    /// Maps its points by `matrix`, as `transform`: exact for affine matrices; with
    /// perspective, curves' control points are mapped (Skia subdivides them).
    pub fn transform(&self, matrix: &Matrix) {
        let data = self.contents.borrow().mapped(&|point| matrix.map(point));
        self.replace(data);
    }

    /// Sets it to `path1` combined with `path2` by `operation`, as `op` (either may be this path:
    /// their contents are taken first).
    ///
    /// Where the platform computes operations ([`set_path_operator`]: desktop, Skia's path ops),
    /// the result is computed now, its contents flat, as Compose's: if it fails, the path is
    /// unchanged and this returns `false`, as Android's. Otherwise (Android, whose path
    /// operations run in its replayer, and headless) the path keeps the operation as steps
    /// (D535), computed by the platform drawing it, and this returns `true`; its fill type is
    /// even-odd, as the result of Skia's path ops. A path combined with itself and another again
    /// and again (a union of many shapes) adds two steps each time, in place.
    pub fn op(&self, path1: &Path, path2: &Path, operation: PathOperation) -> bool {
        if let Some(operator) = PATH_OPERATOR.get() {
            let (first, second) = (path1.data(), path2.data());
            let Some((commands, fill_type)) = operator(operation, &first, &second) else {
                return false;
            };
            drop((first, second));
            self.replace(PathData::operand(commands, fill_type));
            return true;
        }
        // The path combined with another: its steps continue, in place while nothing holds them.
        let into_self = |other: &Path, operation: PathOperation| {
            let other = other.data();
            self.edit(|data| {
                if data.steps.is_empty() {
                    let commands = std::mem::take(&mut data.commands);
                    let operand = PathData::operand(commands, data.fill_type);
                    data.steps.push(PathStep::Push(Arc::new(operand)));
                } else {
                    // The top's fill type, the one its contents have, before it's combined.
                    let fill_type = data.fill_type;
                    let set = match data.steps.last() {
                        Some(PathStep::Combine(_)) => fill_type != PathFillType::EvenOdd,
                        Some(PathStep::SetFillType(last)) => *last != fill_type,
                        _ => true,
                    };
                    if set {
                        data.steps.push(PathStep::SetFillType(fill_type));
                    }
                }
                PathData::push_operand(&mut data.steps, &other);
                data.steps.push(PathStep::Combine(operation));
                data.fill_type = PathFillType::EvenOdd;
                data.current = Offset::ZERO;
                data.contour_start = Offset::ZERO;
                data.has_current = false;
            });
        };
        let simple = |path: &Path| {
            let contents = path.contents.borrow();
            contents.commands.is_empty() || contents.steps.is_empty()
        };
        if path1.ptr_eq(self) && !path2.ptr_eq(self) && simple(self) {
            into_self(path2, operation);
            return true;
        }
        if path2.ptr_eq(self) && !path1.ptr_eq(self) && simple(self) {
            // `other op self` is `self op' other`, for each operation.
            let swapped = match operation {
                PathOperation::Difference => PathOperation::ReverseDifference,
                PathOperation::ReverseDifference => PathOperation::Difference,
                other => other,
            };
            into_self(path1, swapped);
            return true;
        }
        // If the platform can't combine them, the path stays as it was, as Android's.
        let (before, first, second) = (self.data(), path1.data(), path2.data());
        let mut data = PathData::with_id(0);
        PathData::push_operand(&mut data.steps, &before);
        PathData::push_operand(&mut data.steps, &first);
        PathData::push_operand(&mut data.steps, &second);
        data.steps.push(PathStep::CombineOr(operation));
        data.fill_type = PathFillType::EvenOdd;
        drop((before, first, second));
        self.replace(data);
        true
    }

    /// A new path, `path1` combined with `path2` by `operation`, as `Path.combine`.
    pub fn combine(operation: PathOperation, path1: &Path, path2: &Path) -> Path {
        let path = Path::new();
        path.op(path1, path2, operation);
        path
    }

    /// Parses SVG path data (`M`, `L`, `H`, `V`, `C`, `S`, `Q`, `T`, `A`, `Z`, absolute and
    /// relative, numbers separated by spaces, commas or signs), as Compose's `PathParser`.
    pub fn from_svg(data: &str) -> Result<Path, PathError> {
        parse_svg(data).map(Path::from_data)
    }
}

fn parse_svg(data: &str) -> Result<PathData, PathError> {
    Parser { input: data.as_bytes(), position: 0 }.parse()
}

/// SVG path data that couldn't be parsed: the byte offset it failed at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PathError {
    pub position: usize,
}

impl std::fmt::Display for PathError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid SVG path data at byte {}", self.position)
    }
}

impl std::error::Error for PathError {}

struct Parser<'a> {
    input: &'a [u8],
    position: usize,
}

impl Parser<'_> {
    fn skip_separators(&mut self) {
        while self.position < self.input.len()
            && matches!(self.input[self.position], b' ' | b',' | b'\t' | b'\n' | b'\r')
        {
            self.position += 1;
        }
    }

    fn error(&self) -> PathError {
        PathError { position: self.position }
    }

    /// Whether a number comes next.
    fn at_number(&mut self) -> bool {
        self.skip_separators();
        self.input
            .get(self.position)
            .is_some_and(|byte| byte.is_ascii_digit() || matches!(byte, b'-' | b'+' | b'.'))
    }

    fn number(&mut self) -> Result<f32, PathError> {
        self.skip_separators();
        let start = self.position;
        let bytes = self.input;
        let mut end = start;
        if end < bytes.len() && matches!(bytes[end], b'-' | b'+') {
            end += 1;
        }
        let mut seen_dot = false;
        let mut seen_digit = false;
        while end < bytes.len() {
            match bytes[end] {
                b'0'..=b'9' => seen_digit = true,
                // A second dot starts the next number (`0.5.5` is 0.5 and .5).
                b'.' if !seen_dot => seen_dot = true,
                b'e' | b'E' if seen_digit => {
                    if end + 1 < bytes.len() && matches!(bytes[end + 1], b'-' | b'+') {
                        end += 1;
                    }
                }
                _ => break,
            }
            end += 1;
        }
        if !seen_digit {
            return Err(self.error());
        }
        let text = std::str::from_utf8(&bytes[start..end]).map_err(|_| self.error())?;
        self.position = end;
        text.parse().map_err(|_| PathError { position: start })
    }

    /// An arc's flag: a single `0` or `1`, which may run into the next number (`a1 1 0 01.5 2`).
    fn flag(&mut self) -> Result<bool, PathError> {
        self.skip_separators();
        match self.input.get(self.position) {
            Some(b'0') => {
                self.position += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.position += 1;
                Ok(true)
            }
            _ => Err(self.error()),
        }
    }

    fn parse(mut self) -> Result<PathData, PathError> {
        let mut path = PathData::empty();
        let mut current = Offset::ZERO;
        let mut start = Offset::ZERO;
        // The last control point, for `S` and `T`'s reflections.
        let mut last_cubic: Option<Offset> = None;
        let mut last_quad: Option<Offset> = None;
        let mut command = None;
        loop {
            self.skip_separators();
            if self.position >= self.input.len() {
                break;
            }
            let byte = self.input[self.position];
            if byte.is_ascii_alphabetic() {
                command = Some(byte);
                self.position += 1;
            } else if command.is_none() {
                return Err(self.error());
            }
            let Some(letter) = command else { return Err(self.error()) };
            let relative = letter.is_ascii_lowercase();
            let base = if relative { current } else { Offset::ZERO };
            let point = |parser: &mut Parser<'_>| -> Result<Offset, PathError> {
                let x = parser.number()?;
                let y = parser.number()?;
                Ok(Offset::new(x + base.x, y + base.y))
            };
            let (mut next_cubic, mut next_quad) = (None, None);
            match letter.to_ascii_uppercase() {
                b'M' => {
                    current = point(&mut self)?;
                    start = current;
                    path.push(PathCommand::MoveTo(current));
                    // Further pairs are lines.
                    command = Some(if relative { b'l' } else { b'L' });
                }
                b'L' => {
                    current = point(&mut self)?;
                    path.push(PathCommand::LineTo(current));
                }
                b'H' => {
                    let x = self.number()?;
                    current = Offset::new(if relative { current.x + x } else { x }, current.y);
                    path.push(PathCommand::LineTo(current));
                }
                b'V' => {
                    let y = self.number()?;
                    current = Offset::new(current.x, if relative { current.y + y } else { y });
                    path.push(PathCommand::LineTo(current));
                }
                b'C' => {
                    let first = point(&mut self)?;
                    let second = point(&mut self)?;
                    current = point(&mut self)?;
                    path.push(PathCommand::CubicTo(first, second, current));
                    next_cubic = Some(second);
                }
                b'S' => {
                    let first = match last_cubic {
                        Some(control) => current + (current - control),
                        None => current,
                    };
                    let second = point(&mut self)?;
                    current = point(&mut self)?;
                    path.push(PathCommand::CubicTo(first, second, current));
                    next_cubic = Some(second);
                }
                b'Q' => {
                    let control = point(&mut self)?;
                    current = point(&mut self)?;
                    path.push(PathCommand::QuadTo(control, current));
                    next_quad = Some(control);
                }
                b'T' => {
                    let control = match last_quad {
                        Some(control) => current + (current - control),
                        None => current,
                    };
                    current = point(&mut self)?;
                    path.push(PathCommand::QuadTo(control, current));
                    next_quad = Some(control);
                }
                b'A' => {
                    let radius_x = self.number()?;
                    let radius_y = self.number()?;
                    let rotation = self.number()?;
                    let large_arc = self.flag()?;
                    let sweep = self.flag()?;
                    let end = point(&mut self)?;
                    svg_arc(
                        &mut path, current, end, radius_x, radius_y, rotation, large_arc, sweep,
                    );
                    current = end;
                }
                b'Z' => {
                    path.push(PathCommand::Close);
                    current = start;
                    command = None;
                }
                _ => return Err(PathError { position: self.position - 1 }),
            }
            last_cubic = next_cubic;
            last_quad = next_quad;
            // A command letter needs its numbers; after `Z` a letter must follow.
            if command.is_some() && !self.at_number() {
                command = None;
            }
        }
        Ok(path)
    }
}

/// An SVG elliptical arc from `from` to `to`, as cubic Béziers (each covering at most a quarter
/// turn), from the SVG spec's endpoint-to-center conversion.
#[allow(clippy::too_many_arguments)]
fn svg_arc(
    path: &mut PathData,
    from: Offset,
    to: Offset,
    radius_x: f32,
    radius_y: f32,
    rotation: f32,
    large_arc: bool,
    sweep: bool,
) {
    let (mut rx, mut ry) = (f64::from(radius_x.abs()), f64::from(radius_y.abs()));
    if from == to {
        return;
    }
    if rx == 0.0 || ry == 0.0 {
        path.push(PathCommand::LineTo(to));
        return;
    }
    let phi = f64::from(rotation).to_radians();
    let (sin, cos) = phi.sin_cos();
    let (x1, y1) = (f64::from(from.x), f64::from(from.y));
    let (x2, y2) = (f64::from(to.x), f64::from(to.y));
    let dx = (x1 - x2) / 2.0;
    let dy = (y1 - y2) / 2.0;
    let x1p = cos * dx + sin * dy;
    let y1p = -sin * dx + cos * dy;
    // Radii too small to reach: scaled up.
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let numerator = (rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p).max(0.0);
    let denominator = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut factor = (numerator / denominator).sqrt();
    if large_arc == sweep {
        factor = -factor;
    }
    let cxp = factor * rx * y1p / ry;
    let cyp = -factor * ry * x1p / rx;
    let cx = cos * cxp - sin * cyp + (x1 + x2) / 2.0;
    let cy = sin * cxp + cos * cyp + (y1 + y2) / 2.0;
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| {
        let dot = ux * vx + uy * vy;
        let length = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        let mut angle = (dot / length).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            angle = -angle;
        }
        angle
    };
    let start_angle = angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut delta = angle((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry);
    if !sweep && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f64::consts::TAU;
    }
    let segments = (delta.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = delta / segments as f64;
    let handle = 4.0 / 3.0 * (step / 4.0).tan();
    let point_at = |theta: f64| {
        let (sin_t, cos_t) = theta.sin_cos();
        (cx + rx * cos * cos_t - ry * sin * sin_t, cy + rx * sin * cos_t + ry * cos * sin_t)
    };
    let derivative_at = |theta: f64| {
        let (sin_t, cos_t) = theta.sin_cos();
        (-rx * cos * sin_t - ry * sin * cos_t, -rx * sin * sin_t + ry * cos * cos_t)
    };
    let mut theta = start_angle;
    for segment in 0..segments {
        let next = theta + step;
        let (px, py) = point_at(theta);
        let (dx1, dy1) = derivative_at(theta);
        let (qx, qy) = point_at(next);
        let (dx2, dy2) = derivative_at(next);
        let end = if segment + 1 == segments { to } else { Offset::new(qx as f32, qy as f32) };
        path.push(PathCommand::CubicTo(
            Offset::new((px + handle * dx1) as f32, (py + handle * dy1) as f32),
            Offset::new((qx - handle * dx2) as f32, (qy - handle * dy2) as f32),
            end,
        ));
        theta = next;
    }
}

/// A vector image, as Compose's `ImageVector`: paths in a viewport, drawn at a default size.
#[derive(Clone, PartialEq, Debug)]
pub struct ImageVector {
    pub name: &'static str,
    pub default_width: crate::unit::Dp,
    pub default_height: crate::unit::Dp,
    pub viewport_width: f32,
    pub viewport_height: f32,
    /// Each path's contents with its fill (`None`: black, as Material's icons; tinted by
    /// `Icon`). Shared: a clone copies nothing.
    pub paths: Arc<[(Arc<PathData>, Option<crate::graphics::Color>)]>,
}

impl ImageVector {
    /// A 24 dp icon of one path in a 24 × 24 viewport, from SVG path data, as Material's icons.
    pub fn icon(name: &'static str, data: &str) -> ImageVector {
        let path = parse_svg(data).unwrap_or_else(|error| panic!("icon {name}: {error}"));
        ImageVector {
            name,
            default_width: crate::unit::Dp(24.0),
            default_height: crate::unit::Dp(24.0),
            viewport_width: 24.0,
            viewport_height: 24.0,
            paths: Arc::from(vec![(Arc::new(path), None)]),
        }
    }
}

impl compose_runtime::ComposeArg for ImageVector {
    fn same(&self, other: &Self) -> bool {
        self.name == other.name && Arc::ptr_eq(&self.paths, &other.paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_absolute_and_relative_commands() {
        let path = Path::from_svg("M19,13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z").unwrap();
        assert_eq!(path.data().commands()[0], PathCommand::MoveTo(Offset::new(19.0, 13.0)));
        assert_eq!(path.data().commands()[1], PathCommand::LineTo(Offset::new(13.0, 13.0)));
        assert_eq!(path.data().commands()[2], PathCommand::LineTo(Offset::new(13.0, 19.0)));
        assert_eq!(*path.data().commands().last().unwrap(), PathCommand::Close);
        assert_eq!(path.data().commands().len(), 14);
    }

    #[test]
    fn implicit_repeats_and_signs_separate_numbers() {
        let path = Path::from_svg("M9,16.17L4.83,12l-1.42,1.41L9,19 21,7l-1.41-1.41z").unwrap();
        assert_eq!(path.data().commands()[4], PathCommand::LineTo(Offset::new(21.0, 7.0)));
        assert_eq!(path.data().commands()[5], PathCommand::LineTo(Offset::new(19.59, 5.59)));
    }

    #[test]
    fn arcs_become_curves_ending_at_their_end() {
        let path = Path::from_svg("M2,12a10,10 0 1,0 20,0a10,10 0 1,0 -20,0z").unwrap();
        let ends: Vec<Offset> = path
            .data()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PathCommand::CubicTo(_, _, end) => Some(*end),
                _ => None,
            })
            .collect();
        assert_eq!(ends.len(), 4);
        assert_eq!(ends[1], Offset::new(22.0, 12.0));
        assert_eq!(ends[3], Offset::new(2.0, 12.0));
        // A quarter of the circle is on the circle, at the top or the bottom.
        assert!(
            (ends[0].x - 12.0).abs() < 0.01 && ((ends[0].y - 12.0).abs() - 10.0).abs() < 0.01,
            "{ends:?}"
        );
    }

    #[test]
    fn bad_data_says_where() {
        assert_eq!(Path::from_svg("M1,2 X3").unwrap_err().position, 5);
        assert!(Path::from_svg("1,2").is_err());
    }

    /// Every change gives the path a new generation, its identity kept; reading it doesn't.
    #[test]
    fn each_change_bumps_the_generation_and_keeps_the_identity() {
        let path = Path::new();
        let (id, mut last) = (path.id(), path.generation());
        let changes: [&dyn Fn(&Path); 9] = [
            &|path| path.move_to(1.0, 2.0),
            &|path| path.line_to(3.0, 4.0),
            &|path| path.quadratic_to(1.0, 1.0, 2.0, 2.0),
            &|path| path.cubic_to(1.0, 1.0, 2.0, 2.0, 3.0, 3.0),
            &|path| path.close(),
            &|path| path.set_fill_type(PathFillType::EvenOdd),
            &|path| path.add_rect(Rect::new(0.0, 0.0, 1.0, 1.0)),
            &|path| path.reset(),
            &|path| path.rewind(),
        ];
        for change in changes {
            change(&path);
            assert!(path.generation() > last);
            assert_eq!(path.id(), id);
            last = path.generation();
        }
        let _ = (path.get_bounds(), path.is_empty(), path.fill_type(), path.data());
        assert_eq!(path.generation(), last);
        assert_ne!(Path::new().id(), id, "another path is another identity");
        assert_eq!(path.clone().id(), id, "a clone is the same path");
    }

    /// Contents taken before a change keep what they held (a recording's), and the change
    /// doesn't copy what it drops: a reset after drawing gets new contents of the same capacity.
    #[test]
    fn contents_taken_never_change() {
        let path = Path::new();
        path.move_to(0.0, 0.0);
        path.line_to(10.0, 0.0);
        let drawn = path.data();
        path.line_to(10.0, 10.0);
        assert_eq!(drawn.commands().len(), 2);
        assert_eq!(path.data().commands().len(), 3);
        assert_eq!((drawn.id(), drawn.generation() < path.generation()), (path.id(), true));
        let again = path.data();
        path.reset();
        assert_eq!(again.commands().len(), 3, "a reset leaves the drawn contents");
        assert!(path.is_empty());
        // Nothing holds them: changed in place.
        let alone = Arc::as_ptr(&path.data());
        path.move_to(1.0, 1.0);
        assert_eq!(Arc::as_ptr(&path.data()), alone);
    }

    /// `reset` keeps the fill type, as Android's; `rewind` sets it back to non-zero, as Skia's.
    #[test]
    fn reset_keeps_the_fill_type_and_rewind_does_not() {
        let path = Path::new();
        path.set_fill_type(PathFillType::EvenOdd);
        path.add_oval(Rect::new(0.0, 0.0, 4.0, 4.0));
        path.reset();
        assert_eq!(path.fill_type(), PathFillType::EvenOdd);
        assert!(path.is_empty());
        path.rewind();
        assert_eq!(path.fill_type(), PathFillType::NonZero);
    }

    /// Relative commands start at the current point: the last end, or a closed contour's start.
    #[test]
    fn relative_commands_start_at_the_current_point() {
        let path = Path::new();
        path.move_to(10.0, 10.0);
        path.relative_line_to(5.0, 0.0);
        path.relative_quadratic_to(1.0, 1.0, 2.0, 2.0);
        path.relative_cubic_to(1.0, 0.0, 2.0, 0.0, 3.0, 0.0);
        path.close();
        path.relative_move_to(1.0, 1.0);
        assert_eq!(
            path.data().commands(),
            [
                PathCommand::MoveTo(Offset::new(10.0, 10.0)),
                PathCommand::LineTo(Offset::new(15.0, 10.0)),
                PathCommand::QuadTo(Offset::new(16.0, 11.0), Offset::new(17.0, 12.0)),
                PathCommand::CubicTo(
                    Offset::new(18.0, 12.0),
                    Offset::new(19.0, 12.0),
                    Offset::new(20.0, 12.0)
                ),
                PathCommand::Close,
                PathCommand::MoveTo(Offset::new(11.0, 11.0)),
            ]
        );
    }

    /// The ends of each curve and line of `path`.
    fn ends(path: &Path) -> Vec<Offset> {
        path.data()
            .commands()
            .iter()
            .filter_map(|command| match command {
                PathCommand::MoveTo(end)
                | PathCommand::LineTo(end)
                | PathCommand::QuadTo(_, end)
                | PathCommand::CubicTo(_, _, end) => Some(*end),
                PathCommand::Close => None,
            })
            .collect()
    }

    fn near(a: Offset, b: Offset) -> bool {
        (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
    }

    /// Rectangles start at their top left corner and go round as asked; ovals start at their
    /// rightmost point.
    #[test]
    fn shapes_wind_as_asked() {
        let rect = Rect::new(0.0, 0.0, 10.0, 20.0);
        let path = Path::new();
        path.add_rect_with(rect, PathDirection::Clockwise);
        assert_eq!(
            ends(&path),
            [(0.0, 0.0), (10.0, 0.0), (10.0, 20.0), (0.0, 20.0)].map(|(x, y)| Offset::new(x, y))
        );
        path.reset();
        path.add_rect(rect);
        assert_eq!(ends(&path)[1], Offset::new(0.0, 20.0), "counterclockwise by default");
        path.reset();
        path.add_oval_with(rect, PathDirection::Clockwise);
        let points = ends(&path);
        assert_eq!(points.len(), 5);
        for (point, expected) in
            points.iter().zip([(10.0, 10.0), (5.0, 20.0), (0.0, 10.0), (5.0, 0.0)])
        {
            assert!(near(*point, Offset::new(expected.0, expected.1)), "{points:?}");
        }
        assert_eq!(path.data().commands().last(), Some(&PathCommand::Close));
        assert_eq!(path.get_bounds(), rect, "the curves' control points stay in the oval");
    }

    /// A round rectangle's corners are quarter ellipses, radii too large scaled down together;
    /// square corners are lines.
    #[test]
    fn round_rects_scale_radii_that_dont_fit() {
        let path = Path::new();
        let round = RoundRect::new(Rect::new(0.0, 0.0, 10.0, 10.0), CornerRadius::new(10.0, 10.0));
        path.add_round_rect_with(round, PathDirection::Clockwise);
        // Radii of 10 on a 10 px side: scaled to 5, a circle.
        let points = ends(&path);
        for expected in [(5.0, 0.0), (10.0, 5.0), (5.0, 10.0), (0.0, 5.0)] {
            assert!(points.iter().any(|point| near(*point, Offset::new(expected.0, expected.1))));
        }
        path.reset();
        let square = RoundRect::new(Rect::new(0.0, 0.0, 10.0, 10.0), CornerRadius::ZERO);
        path.add_round_rect(square);
        let mut drawn = path.data().commands().to_vec();
        drawn.pop();
        assert!(drawn.iter().all(|command| !matches!(command, PathCommand::CubicTo(..))));
    }

    /// An arc lines from the current point to its start, or moves there when forced or first.
    #[test]
    fn arcs_join_the_contour_unless_forced() {
        let oval = Rect::new(0.0, 0.0, 20.0, 20.0);
        let path = Path::new();
        path.arc_to(oval, 0.0, 90.0, false);
        assert_eq!(path.data().commands()[0], PathCommand::MoveTo(Offset::new(20.0, 10.0)));
        assert!(near(*ends(&path).last().unwrap(), Offset::new(10.0, 20.0)));
        path.reset();
        path.move_to(0.0, 0.0);
        path.arc_to(oval, 0.0, 90.0, false);
        assert_eq!(path.data().commands()[1], PathCommand::LineTo(Offset::new(20.0, 10.0)));
        path.arc_to(oval, 180.0, -90.0, true);
        assert!(matches!(path.data().commands()[3], PathCommand::MoveTo(_)));
    }

    /// The kinds of `data`'s steps: 'p'ush, 'c'ombine, combine 'o'r, 'a'dd, 'f'ill.
    fn kinds(data: &PathData) -> String {
        data.steps()
            .iter()
            .map(|step| match step {
                PathStep::Push(_) => 'p',
                PathStep::Combine(_) => 'c',
                PathStep::CombineOr(_) => 'o',
                PathStep::Add => 'a',
                PathStep::SetFillType(_) => 'f',
            })
            .collect()
    }

    fn rect(left: f32, top: f32, right: f32, bottom: f32) -> Path {
        let path = Path::new();
        path.add_rect(Rect::new(left, top, right, bottom));
        path
    }

    /// Without the platform's path operations (no `PathOperator`: Android, headless), an
    /// operation keeps its operands' contents as they were as steps (D535), and bounds its result
    /// by them.
    #[test]
    fn an_operation_keeps_its_operands_as_steps() {
        let (first, second) = (rect(0.0, 0.0, 10.0, 10.0), rect(5.0, 5.0, 20.0, 20.0));
        let path = Path::combine(PathOperation::Intersect, &first, &second);
        first.reset();
        let data = path.data();
        // The path's contents before (empty), kept if the platform can't combine them.
        assert_eq!(kinds(&data), "pppo");
        let PathStep::Push(operand) = &data.steps()[1] else { panic!() };
        assert_eq!((operand.commands().len(), operand.id()), (5, first.id()), "kept, shared");
        assert_eq!(data.steps()[3], PathStep::CombineOr(PathOperation::Intersect));
        assert_eq!(path.fill_type(), PathFillType::EvenOdd);
        assert_eq!(path.get_bounds(), Rect::new(5.0, 5.0, 10.0, 10.0));
        // The path itself as an operand.
        let square = rect(0.0, 0.0, 4.0, 4.0);
        assert!(square.op(&square, &second, PathOperation::Union));
        assert_eq!(square.get_bounds(), Rect::new(0.0, 0.0, 20.0, 20.0));
        square.line_to(30.0, 30.0);
        assert_eq!(square.get_bounds(), Rect::new(0.0, 0.0, 30.0, 30.0));
    }

    /// A path combined with itself and another again and again (a union of many shapes) adds two
    /// steps each time, in place when nothing holds its contents: the steps stay a flat list,
    /// however long, and the other side's operation is flipped (`x - path` is `path` reversed
    /// less `x`).
    #[test]
    fn a_chain_of_operations_stays_flat() {
        let (union, shape) = (Path::new(), Path::new());
        let mut pointer = None;
        for index in 0..1000 {
            shape.reset();
            shape.add_rect(Rect::new(index as f32, 0.0, index as f32 + 1.0, 1.0));
            union.op(&union, &shape, PathOperation::Union);
            if index == 10 {
                pointer = Some(Arc::as_ptr(&union.data()));
            }
        }
        let data = union.data();
        assert_eq!(data.steps().len(), 1 + 2 * 1000);
        assert_eq!(Arc::as_ptr(&data), pointer.unwrap(), "changed in place");
        assert!(data.steps().iter().all(|step| match step {
            PathStep::Push(operand) => operand.steps().is_empty(),
            _ => true,
        }));
        assert_eq!(union.get_bounds(), Rect::new(0.0, 0.0, 1000.0, 1.0));
        let hole = rect(2.0, 0.0, 3.0, 1.0);
        hole.op(&union, &hole, PathOperation::Difference);
        // The hole, then the union's steps inlined and its fill type set, then the operation the
        // other way round.
        assert_eq!(kinds(&hole.data()).len(), 1 + 2001 + 2);
        assert_eq!(
            hole.data().steps().last(),
            Some(&PathStep::Combine(PathOperation::ReverseDifference))
        );
        let base = rect(0.0, 0.0, 1.0, 1.0);
        let other = rect(5.0, 5.0, 6.0, 6.0);
        base.op(&other, &base, PathOperation::Difference);
        assert_eq!(base.data().steps()[2], PathStep::Combine(PathOperation::ReverseDifference));
    }

    /// An operation's result is known to be empty where its operands' bounds say it is.
    #[test]
    fn an_operation_is_empty_when_its_operands_say_so() {
        let (a, b) = (rect(0.0, 0.0, 1.0, 1.0), rect(5.0, 5.0, 6.0, 6.0));
        let empty = Path::new();
        assert!(Path::combine(PathOperation::Intersect, &a, &b).is_empty());
        assert!(!Path::combine(PathOperation::Union, &a, &b).is_empty());
        assert!(Path::combine(PathOperation::Difference, &empty, &a).is_empty());
        assert!(!Path::combine(PathOperation::Difference, &a, &b).is_empty());
        assert!(Path::combine(PathOperation::Union, &empty, &empty).is_empty());
    }

    /// `add_path` adds another path's contours moved by an offset; `translate` and `transform`
    /// move every point, an operation's operands too.
    #[test]
    fn paths_add_and_move() {
        let (path, other) = (rect(0.0, 0.0, 1.0, 1.0), rect(0.0, 0.0, 2.0, 2.0));
        path.add_path(&other, Offset::new(10.0, 0.0));
        assert_eq!(path.get_bounds(), Rect::new(0.0, 0.0, 12.0, 2.0));
        assert_eq!(path.data().commands().len(), 10);
        path.translate(Offset::new(1.0, 1.0));
        assert_eq!(path.get_bounds(), Rect::new(1.0, 1.0, 13.0, 3.0));
        path.transform(&Matrix::scaling(2.0, 3.0));
        assert_eq!(path.get_bounds(), Rect::new(2.0, 3.0, 26.0, 9.0));
        let combined = Path::combine(PathOperation::Union, &rect(0.0, 0.0, 1.0, 1.0), &other);
        combined.translate(Offset::new(5.0, 5.0));
        assert_eq!(combined.get_bounds(), Rect::new(5.0, 5.0, 7.0, 7.0));
        let added = rect(20.0, 20.0, 21.0, 21.0);
        added.add_path(&combined, Offset::ZERO);
        assert_eq!(kinds(&added.data()), "ppppofa");
        assert_eq!(added.get_bounds(), Rect::new(5.0, 5.0, 21.0, 21.0));
        let turned = rect(0.0, 0.0, 2.0, 1.0);
        turned.transform(&Matrix::rotation(90.0));
        let bounds = turned.get_bounds();
        assert!(
            (bounds.left + 1.0).abs() < 1e-5 && (bounds.bottom - 2.0).abs() < 1e-5,
            "{bounds:?}"
        );
    }

    /// Convexity, as `isConvex`: one contour turning one way.
    #[test]
    fn convex_paths_turn_one_way() {
        assert!(rect(0.0, 0.0, 1.0, 1.0).is_convex());
        let oval = Path::new();
        oval.add_oval(Rect::new(0.0, 0.0, 4.0, 2.0));
        assert!(oval.is_convex());
        let arrow = Path::new();
        for (x, y) in [(0.0, 0.0), (10.0, 0.0), (5.0, 5.0), (10.0, 10.0), (0.0, 10.0)] {
            if x == 0.0 && y == 0.0 { arrow.move_to(x, y) } else { arrow.line_to(x, y) }
        }
        arrow.close();
        assert!(!arrow.is_convex());
        let two = rect(0.0, 0.0, 1.0, 1.0);
        two.add_rect(Rect::new(5.0, 5.0, 6.0, 6.0));
        assert!(!two.is_convex());
    }

    /// A whole turn of an arc from a quarter's angle is the closed oval, as Skia's `addArc`; from
    /// another angle a lone point; none for no sweep.
    #[test]
    fn a_whole_arc_is_the_closed_oval() {
        let oval = Rect::new(0.0, 0.0, 10.0, 10.0);
        let arc = Path::new();
        arc.add_arc(oval, 90.0, 360.0);
        assert_eq!(arc.data().commands().last(), Some(&PathCommand::Close));
        assert_eq!(arc.data().commands()[0], PathCommand::MoveTo(Offset::new(5.0, 10.0)));
        // From another angle, Skia's `arcTo`: a lone point at its end.
        let lone = Path::new();
        lone.add_arc(oval, 45.0, 360.0);
        assert_eq!(lone.data().commands().len(), 1);
        assert!(matches!(lone.data().commands()[0], PathCommand::MoveTo(_)));
        let none = Path::new();
        none.add_arc(oval, 0.0, 0.0);
        assert!(none.is_empty());
    }

    /// `q.op(&a, &b)` keeps `q`'s earlier contents for the platform to fall back on: an operation
    /// it can't do leaves `q` as it was, as Android's `Path.op` (a review).
    #[test]
    fn a_failed_operation_leaves_the_path_as_it_was() {
        let (q, a, b) =
            (rect(0.0, 0.0, 1.0, 1.0), rect(0.0, 0.0, 2.0, 2.0), rect(5.0, 5.0, 6.0, 6.0));
        q.line_to(3.0, 3.0);
        q.op(&a, &b, PathOperation::Union);
        let commands = |data: &PathData| data.commands().len();
        let failing = q.data().evaluate_steps(
            commands,
            |first, _, _| first,
            |before, _, _, _| before,
            |first, _| first,
            |_, _| {},
        );
        assert_eq!(failing, Some(7), "q's rectangle and line");
        let succeeding = q.data().evaluate_steps(
            commands,
            |first, _, _| first,
            |_, first, second, _| first + second,
            |first, _| first,
            |_, _| {},
        );
        assert_eq!(succeeding, Some(10));
    }

    /// A contour drawn on after a close starts again, as Skia's: two contours aren't convex.
    #[test]
    fn a_line_after_a_close_is_another_contour() {
        let path = rect(0.0, 0.0, 1.0, 1.0);
        path.line_to(5.0, 5.0);
        assert_eq!(path.data().commands()[5], PathCommand::MoveTo(Offset::ZERO));
        assert!(!path.is_convex());
        assert!(Path::new().is_convex());
    }

    /// `Matrix::map` sends a point at infinity (w = 0) to the origin, as Compose's.
    #[test]
    fn a_point_at_infinity_maps_to_the_origin() {
        let mut matrix = Matrix::new();
        matrix.values[15] = 0.0;
        assert_eq!(matrix.map(Offset::new(3.0, 4.0)), Offset::ZERO);
        assert_eq!(Matrix::translation(1.0, 2.0).map(Offset::new(3.0, 4.0)), Offset::new(4.0, 6.0));
    }
}
