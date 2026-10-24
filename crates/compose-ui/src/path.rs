//! Paths, as Compose's `Path`: lines, curves, shapes and closes, filled or stroked, built in code
//! or parsed from SVG path data (`PathParser`), which is how vector icons ship.
//!
//! A [`Path`] is mutable and shared, as Compose's (D530): remembered once and reset in a drawing
//! block each frame, as Compose code does. Drawing it records its contents as they are then, a
//! [`PathData`] that never changes once shared: a recording keeps what it drew, as a render node
//! keeps its copy of an `android.graphics.Path`. Each change gives the path a new generation, so
//! a platform keeping a path by identity (Android's replayer) redefines it in place, and a
//! drawing whose path changed after it was recorded is drawn again (D531).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Weak;
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

/// Each path's identity: a path keeps it while it changes (Android's replayer keeps one
/// `android.graphics.Path` per identity and generation it was drawn at).
static NEXT_IDENTITY: AtomicU64 = AtomicU64::new(1);

/// What a path holds at one generation: its commands, fill type, and the operation it starts
/// with, if any. Shared (`Arc`) by what drew it, it never changes: a [`Path`] changed after
/// drawing gets new contents, with the same [`id`](PathData::id) and a new
/// [`generation`](PathData::generation).
#[derive(Clone, Debug)]
pub struct PathData {
    id: u64,
    generation: u64,
    fill_type: PathFillType,
    operands: Option<Arc<PathOperands>>,
    commands: Vec<PathCommand>,
    /// Where the next relative command starts: the last command's end.
    current: Offset,
    /// The last contour's start, where a close leaves the current point.
    contour_start: Offset,
    /// A command after this one draws from `current`: `false` until the first command (an
    /// `arc_to` then moves rather than draws a line).
    has_current: bool,
}

/// [`Path::op`]'s operands: the path's contents start with `path1` combined with `path2` by
/// `operation`, which each platform computes (Skia's path ops, `android.graphics.Path.op`), as
/// Compose's `Path.op` leaves to its platform's path.
#[derive(Debug, PartialEq)]
pub struct PathOperands {
    pub operation: PathOperation,
    pub path1: Arc<PathData>,
    pub path2: Arc<PathData>,
}

/// Contents only: two paths' contents are equal whatever their identities and generations.
impl PartialEq for PathData {
    fn eq(&self, other: &Self) -> bool {
        self.fill_type == other.fill_type
            && self.commands == other.commands
            && self.operands == other.operands
    }
}

impl PathData {
    fn empty() -> PathData {
        PathData {
            id: NEXT_IDENTITY.fetch_add(1, Ordering::Relaxed),
            generation: 0,
            fill_type: PathFillType::NonZero,
            operands: None,
            commands: Vec::new(),
            current: Offset::ZERO,
            contour_start: Offset::ZERO,
            has_current: false,
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

    /// The commands, drawn after the operation's result if there is one.
    pub fn commands(&self) -> &[PathCommand] {
        &self.commands
    }

    /// The operation whose result the path starts with ([`Path::op`]).
    pub fn operands(&self) -> Option<&PathOperands> {
        self.operands.as_deref()
    }

    /// Whether it draws nothing: no command, no operation.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty() && self.operands.is_none()
    }

    /// The bounds of its points, curves' control points included, as `getBounds` (Skia's and
    /// Android's: not the curves' tight bounds). An operation's result is bounded by its operands'
    /// bounds as the operation combines them (the first's for a difference, both for an
    /// intersection): its own may be smaller. Empty: all zeros.
    pub fn bounds(&self) -> Rect {
        let mut bounds: Option<Rect> = self.operands.as_ref().and_then(|operands| {
            let (first, second) =
                (operands.path1.bounds_or_none(), operands.path2.bounds_or_none());
            match operands.operation {
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
        });
        for command in &self.commands {
            let points: &[Offset] = match command {
                PathCommand::MoveTo(point) | PathCommand::LineTo(point) => {
                    std::slice::from_ref(point)
                }
                PathCommand::QuadTo(control, end) => &[*control, *end],
                PathCommand::CubicTo(first, second, end) => &[*first, *second, *end],
                PathCommand::Close => &[],
            };
            for point in points {
                let rect = Rect::new(point.x, point.y, point.x, point.y);
                bounds = union(bounds, Some(rect));
            }
        }
        bounds.unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0))
    }

    fn bounds_or_none(&self) -> Option<Rect> {
        (!self.is_empty()).then(|| self.bounds())
    }

    fn push(&mut self, command: PathCommand) {
        match command {
            PathCommand::MoveTo(point) => {
                self.current = point;
                self.contour_start = point;
            }
            PathCommand::LineTo(end)
            | PathCommand::QuadTo(_, end)
            | PathCommand::CubicTo(_, _, end) => {
                if !self.has_current {
                    // As Skia, a contour drawn without a move starts at the origin.
                    self.contour_start = Offset::ZERO;
                }
                self.current = end;
            }
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
/// path reset and rebuilt between two draws, in the same block or another, draws each shape it had.
/// A drawing whose path changed after it was recorded is drawn again in the next frame (D531).
#[derive(Clone)]
pub struct Path {
    cell: Rc<PathCell>,
}

/// What a path's clones share.
struct PathCell {
    contents: RefCell<Arc<PathData>>,
    /// The contents' generation, which a recording that drew them follows without holding the
    /// path (`WeakPath`, `Send` and `Sync` as the display list is).
    generation: Arc<AtomicU64>,
}

impl Default for Path {
    fn default() -> Path {
        Path::new()
    }
}

impl std::fmt::Debug for Path {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("Path").field(&*self.cell.contents.borrow()).finish()
    }
}

/// By identity, as a Kotlin reference: the same path, whatever it holds.
impl compose_runtime::ComposeArg for Path {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.cell, &other.cell)
    }
}

impl Path {
    /// An empty path, filled non-zero, as `Path()`.
    pub fn new() -> Path {
        Path::from_data(PathData::empty())
    }

    fn from_data(data: PathData) -> Path {
        let generation = Arc::new(AtomicU64::new(data.generation));
        Path { cell: Rc::new(PathCell { contents: RefCell::new(Arc::new(data)), generation }) }
    }

    /// Its contents now, shared: what drawing it records. They never change; the path's later
    /// changes are new contents.
    pub fn data(&self) -> Arc<PathData> {
        Arc::clone(&self.cell.contents.borrow())
    }

    /// Its identity: the same for every clone, and kept as it changes.
    pub fn id(&self) -> u64 {
        self.cell.contents.borrow().id
    }

    /// Its generation: changed by each change.
    pub fn generation(&self) -> u64 {
        self.cell.contents.borrow().generation
    }

    /// Whether it is the same path as `other` (a clone of it), as Kotlin's `===`.
    pub fn ptr_eq(&self, other: &Path) -> bool {
        Rc::ptr_eq(&self.cell, &other.cell)
    }

    pub(crate) fn downgrade(&self) -> WeakPath {
        WeakPath(Arc::downgrade(&self.cell.generation))
    }

    /// Changes its contents through `change`, in place when nothing else holds them (nothing
    /// drew them, or what did is gone), else in a copy: what drew them keeps them.
    fn edit<R>(&self, change: impl FnOnce(&mut PathData) -> R) -> R {
        let mut contents = self.cell.contents.borrow_mut();
        if Arc::get_mut(&mut contents).is_none() {
            *contents = Arc::new((**contents).clone());
        }
        let data = Arc::get_mut(&mut contents).expect("unique contents");
        data.generation += 1;
        self.cell.generation.store(data.generation, Ordering::Relaxed);
        change(data)
    }

    /// Empties it, as `edit` changes it, without copying what it held; keeps its fill type if
    /// `keep_fill_type`.
    fn clear(&self, keep_fill_type: bool) {
        let mut contents = self.cell.contents.borrow_mut();
        let fill_type = if keep_fill_type { contents.fill_type } else { PathFillType::NonZero };
        let cleared = |data: &mut PathData| {
            data.generation += 1;
            data.fill_type = fill_type;
            data.operands = None;
            data.commands.clear();
            data.current = Offset::ZERO;
            data.contour_start = Offset::ZERO;
            data.has_current = false;
        };
        match Arc::get_mut(&mut contents) {
            Some(data) => cleared(data),
            None => {
                let mut data = PathData {
                    id: contents.id,
                    generation: contents.generation,
                    fill_type,
                    operands: None,
                    // The room the last contents took: a path rebuilt each frame grows once.
                    commands: Vec::with_capacity(contents.commands.capacity()),
                    current: Offset::ZERO,
                    contour_start: Offset::ZERO,
                    has_current: false,
                };
                cleared(&mut data);
                *contents = Arc::new(data);
            }
        }
        self.cell.generation.store(contents.generation, Ordering::Relaxed);
    }

    /// Which points are inside it, as `fillType`.
    pub fn fill_type(&self) -> PathFillType {
        self.cell.contents.borrow().fill_type
    }

    /// Sets which points are inside it, as `fillType = …`.
    pub fn set_fill_type(&self, fill_type: PathFillType) {
        if self.fill_type() != fill_type {
            self.edit(|data| data.fill_type = fill_type);
        }
    }

    /// Whether it draws nothing, as `isEmpty`.
    pub fn is_empty(&self) -> bool {
        self.cell.contents.borrow().is_empty()
    }

    /// The bounds of its points (curves' control points included), as `getBounds`: see
    /// [`PathData::bounds`].
    pub fn get_bounds(&self) -> Rect {
        self.cell.contents.borrow().bounds()
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
        self.edit(|data| data.arc(rect, start_angle_radians, sweep_angle_radians, force_move_to));
    }

    /// An arc of the oval in `oval` as a contour of its own, as `addArc`.
    pub fn add_arc(&self, oval: Rect, start_angle_degrees: f32, sweep_angle_degrees: f32) {
        self.add_arc_rad(oval, start_angle_degrees.to_radians(), sweep_angle_degrees.to_radians());
    }

    /// [`add_arc`](Path::add_arc) in radians, as `addArcRad`.
    pub fn add_arc_rad(&self, oval: Rect, start_angle_radians: f32, sweep_angle_radians: f32) {
        self.edit(|data| data.arc(oval, start_angle_radians, sweep_angle_radians, true));
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

    /// Sets it to `path1` combined with `path2` by `operation`, as `op` (either may be this path:
    /// their contents are taken first). Its fill type becomes even-odd, as the result of Skia's
    /// path ops. Returns `true`: each platform computes the result when it draws it (an operation
    /// it can't do draws nothing there), where Compose's returns whether its platform could.
    pub fn op(&self, path1: &Path, path2: &Path, operation: PathOperation) -> bool {
        let operands =
            Arc::new(PathOperands { operation, path1: path1.data(), path2: path2.data() });
        self.clear(false);
        self.edit(|data| {
            data.fill_type = PathFillType::EvenOdd;
            data.operands = Some(operands);
        });
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

/// A path that doesn't keep it alive, to ask its generation: what a recording notes of the paths
/// it drew (D531).
#[derive(Clone)]
pub(crate) struct WeakPath(Weak<AtomicU64>);

impl std::fmt::Debug for WeakPath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("WeakPath")
    }
}

impl WeakPath {
    /// Its generation now, or `None` once it's gone (it can't change any more).
    pub(crate) fn generation(&self) -> Option<u64> {
        self.0.upgrade().map(|generation| generation.load(Ordering::Relaxed))
    }

    /// The address it's known by, to sort and deduplicate.
    pub(crate) fn address(&self) -> usize {
        self.0.as_ptr() as usize
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

    /// An operation keeps its operands' contents as they were, and bounds its result by them.
    #[test]
    fn an_operation_keeps_its_operands() {
        let (first, second) = (Path::new(), Path::new());
        first.add_rect(Rect::new(0.0, 0.0, 10.0, 10.0));
        second.add_rect(Rect::new(5.0, 5.0, 20.0, 20.0));
        let path = Path::combine(PathOperation::Intersect, &first, &second);
        first.reset();
        let data = path.data();
        let operands = data.operands().expect("operands");
        assert_eq!(operands.operation, PathOperation::Intersect);
        assert_eq!(operands.path1.commands().len(), 5);
        assert_eq!(path.fill_type(), PathFillType::EvenOdd);
        assert_eq!(path.get_bounds(), Rect::new(5.0, 5.0, 10.0, 10.0));
        // The path itself as an operand.
        let rect = Path::new();
        rect.add_rect(Rect::new(0.0, 0.0, 4.0, 4.0));
        assert!(rect.op(&rect, &second, PathOperation::Union));
        assert_eq!(rect.get_bounds(), Rect::new(0.0, 0.0, 20.0, 20.0));
        rect.line_to(30.0, 30.0);
        assert_eq!(rect.get_bounds(), Rect::new(0.0, 0.0, 30.0, 30.0));
    }
}
