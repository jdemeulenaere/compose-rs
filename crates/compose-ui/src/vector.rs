//! Vector graphics, as Compose's `androidx.compose.ui.graphics.vector`: path data as
//! [`PathNode`]s, parsed from SVG path strings ([`PathParser`]) or built ([`PathBuilder`]);
//! [`ImageVector`]s of groups and paths, built with an [`ImageVectorBuilder`]; and the
//! [`VectorPainter`] drawing one.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use crate::alignment::LayoutDirection;
use crate::brush::Brush;
use crate::color_filter::{BlendMode, ColorFilter};
use crate::display_list::{DisplayList, DrawOp};
use crate::draw::DrawScope;
use crate::graphics::{Color, Paint, PaintStyle, StrokeCap, StrokeJoin};
use crate::painter::{CustomPainter, Painter};
use crate::path::{PathCommand, PathData, PathError, PathFillType};
use crate::unit::{Density, Dp, Offset, Rect, Size};

/// One command of path data, as Compose's `PathNode`: the SVG path commands, absolute and
/// relative.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PathNode {
    Close,
    RelativeMoveTo {
        dx: f32,
        dy: f32,
    },
    MoveTo {
        x: f32,
        y: f32,
    },
    RelativeLineTo {
        dx: f32,
        dy: f32,
    },
    LineTo {
        x: f32,
        y: f32,
    },
    RelativeHorizontalTo {
        dx: f32,
    },
    HorizontalTo {
        x: f32,
    },
    RelativeVerticalTo {
        dy: f32,
    },
    VerticalTo {
        y: f32,
    },
    RelativeCurveTo {
        dx1: f32,
        dy1: f32,
        dx2: f32,
        dy2: f32,
        dx3: f32,
        dy3: f32,
    },
    CurveTo {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
    },
    RelativeReflectiveCurveTo {
        dx1: f32,
        dy1: f32,
        dx2: f32,
        dy2: f32,
    },
    ReflectiveCurveTo {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },
    RelativeQuadTo {
        dx1: f32,
        dy1: f32,
        dx2: f32,
        dy2: f32,
    },
    QuadTo {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },
    RelativeReflectiveQuadTo {
        dx: f32,
        dy: f32,
    },
    ReflectiveQuadTo {
        x: f32,
        y: f32,
    },
    RelativeArcTo {
        horizontal_ellipse_radius: f32,
        vertical_ellipse_radius: f32,
        theta: f32,
        is_more_than_half: bool,
        is_positive_arc: bool,
        arc_start_dx: f32,
        arc_start_dy: f32,
    },
    ArcTo {
        horizontal_ellipse_radius: f32,
        vertical_ellipse_radius: f32,
        theta: f32,
        is_more_than_half: bool,
        is_positive_arc: bool,
        arc_start_x: f32,
        arc_start_y: f32,
    },
}

/// Parses SVG path data into [`PathNode`]s, as Compose's `PathParser`: lenient as Compose's
/// (any run of spaces and commas separates numbers, a sign or a second dot starts the next
/// one, an arc's flags may run into what follows); an unknown command letter is an error.
#[derive(Default, Debug)]
pub struct PathParser {
    nodes: Vec<PathNode>,
}

impl PathParser {
    pub fn new() -> PathParser {
        PathParser::default()
    }

    /// Forgets the nodes parsed, as `clear`.
    pub fn clear(&mut self) {
        self.nodes.clear();
    }

    /// Parses `path_data`, its nodes after those already parsed, as `parsePathString`.
    pub fn parse_path_string(&mut self, path_data: &str) -> Result<&mut PathParser, PathError> {
        parse_nodes(path_data, &mut self.nodes)?;
        Ok(self)
    }

    /// Adds `nodes`, as `addPathNodes`.
    pub fn add_path_nodes(&mut self, nodes: &[PathNode]) -> &mut PathParser {
        self.nodes.extend_from_slice(nodes);
        self
    }

    /// The nodes parsed, as `toNodes`.
    pub fn to_nodes(&self) -> Vec<PathNode> {
        self.nodes.clone()
    }

    /// The nodes as a path, as `toPath`.
    pub fn to_path(&self) -> crate::path::Path {
        nodes_to_path(&self.nodes)
    }
}

/// The nodes of SVG path data, as Compose's `addPathNodes(pathStr)`.
///
/// # Panics
///
/// On an unknown command letter, as Compose throws.
pub fn add_path_nodes(path_data: &str) -> Vec<PathNode> {
    let mut nodes = Vec::new();
    parse_nodes(path_data, &mut nodes)
        .unwrap_or_else(|error| panic!("compose-ui: path data {path_data:?}: {error}"));
    nodes
}

/// `nodes` as a path, as Compose's `List<PathNode>.toPath()`.
pub fn nodes_to_path(nodes: &[PathNode]) -> crate::path::Path {
    crate::path::Path::from_shared_data(Arc::new(nodes_to_data(nodes, PathFillType::NonZero)))
}

/// Reads a number at `at`: an optional sign, digits with at most one dot, an exponent.
fn read_number(bytes: &[u8], at: usize) -> Option<(f32, usize)> {
    let mut end = at;
    if end < bytes.len() && matches!(bytes[end], b'-' | b'+') {
        end += 1;
    }
    let (mut dot, mut digit) = (false, false);
    while end < bytes.len() {
        match bytes[end] {
            b'0'..=b'9' => digit = true,
            b'.' if !dot => dot = true,
            b'e' | b'E' if digit => {
                if end + 1 < bytes.len() && matches!(bytes[end + 1], b'-' | b'+') {
                    end += 1;
                }
            }
            _ => break,
        }
        end += 1;
    }
    if !digit {
        return None;
    }
    std::str::from_utf8(&bytes[at..end]).ok()?.parse().ok().map(|value| (value, end))
}

fn parse_nodes(data: &str, nodes: &mut Vec<PathNode>) -> Result<(), PathError> {
    let bytes = data.as_bytes();
    let mut index = 0;
    let mut args: Vec<f32> = Vec::with_capacity(8);
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        let lower = byte | 0x20;
        if !(lower.is_ascii_lowercase() && lower != b'e') {
            continue;
        }
        args.clear();
        if lower != b'z' {
            let separator = |byte: u8| byte <= b' ' || byte == b',';
            while index < bytes.len() && separator(bytes[index]) {
                index += 1;
            }
            loop {
                // An arc's two flags are one character each, maybe run into the next number.
                let flag = lower == b'a' && matches!(args.len() % 7, 3 | 4);
                let read = if flag {
                    match bytes.get(index) {
                        Some(b'0') => Some((0.0, index + 1)),
                        Some(b'1') => Some((1.0, index + 1)),
                        _ => None,
                    }
                } else {
                    read_number(bytes, index)
                };
                let Some((value, end)) = read else { break };
                args.push(value);
                index = end;
                while index < bytes.len() && separator(bytes[index]) {
                    index += 1;
                }
            }
        }
        add_nodes(byte, &args, nodes).ok_or(PathError { position: index - 1 })?;
    }
    Ok(())
}

/// The nodes of command `command` with `args` (as many whole groups as there are).
fn add_nodes(command: u8, args: &[f32], nodes: &mut Vec<PathNode>) -> Option<()> {
    use PathNode::*;
    let arity = match command | 0x20 {
        b'z' => {
            nodes.push(Close);
            return Some(());
        }
        b'h' | b'v' => 1,
        b'm' | b'l' | b't' => 2,
        b's' | b'q' => 4,
        b'c' => 6,
        b'a' => 7,
        _ => return None,
    };
    for (group, a) in args.chunks_exact(arity).enumerate() {
        nodes.push(match command {
            b'm' if group > 0 => RelativeLineTo { dx: a[0], dy: a[1] },
            b'M' if group > 0 => LineTo { x: a[0], y: a[1] },
            b'm' => RelativeMoveTo { dx: a[0], dy: a[1] },
            b'M' => MoveTo { x: a[0], y: a[1] },
            b'l' => RelativeLineTo { dx: a[0], dy: a[1] },
            b'L' => LineTo { x: a[0], y: a[1] },
            b'h' => RelativeHorizontalTo { dx: a[0] },
            b'H' => HorizontalTo { x: a[0] },
            b'v' => RelativeVerticalTo { dy: a[0] },
            b'V' => VerticalTo { y: a[0] },
            b'c' => {
                RelativeCurveTo { dx1: a[0], dy1: a[1], dx2: a[2], dy2: a[3], dx3: a[4], dy3: a[5] }
            }
            b'C' => CurveTo { x1: a[0], y1: a[1], x2: a[2], y2: a[3], x3: a[4], y3: a[5] },
            b's' => RelativeReflectiveCurveTo { dx1: a[0], dy1: a[1], dx2: a[2], dy2: a[3] },
            b'S' => ReflectiveCurveTo { x1: a[0], y1: a[1], x2: a[2], y2: a[3] },
            b'q' => RelativeQuadTo { dx1: a[0], dy1: a[1], dx2: a[2], dy2: a[3] },
            b'Q' => QuadTo { x1: a[0], y1: a[1], x2: a[2], y2: a[3] },
            b't' => RelativeReflectiveQuadTo { dx: a[0], dy: a[1] },
            b'T' => ReflectiveQuadTo { x: a[0], y: a[1] },
            b'a' => RelativeArcTo {
                horizontal_ellipse_radius: a[0],
                vertical_ellipse_radius: a[1],
                theta: a[2],
                is_more_than_half: a[3] != 0.0,
                is_positive_arc: a[4] != 0.0,
                arc_start_dx: a[5],
                arc_start_dy: a[6],
            },
            _ => ArcTo {
                horizontal_ellipse_radius: a[0],
                vertical_ellipse_radius: a[1],
                theta: a[2],
                is_more_than_half: a[3] != 0.0,
                is_positive_arc: a[4] != 0.0,
                arc_start_x: a[5],
                arc_start_y: a[6],
            },
        });
    }
    Some(())
}

/// The contents of `nodes`: lines, curves (a reflective one reflecting the last control point
/// when it follows one of its kind, else starting at the current point), arcs as cubic
/// curves, as Compose's `toPath`.
pub(crate) fn nodes_to_data(nodes: &[PathNode], fill_type: PathFillType) -> PathData {
    use PathNode::*;
    let mut data = PathData::empty();
    data.set_fill_type(fill_type);
    let (mut current, mut start) = (Offset::ZERO, Offset::ZERO);
    let (mut last_cubic, mut last_quad): (Option<Offset>, Option<Offset>) = (None, None);
    for node in nodes {
        let base = current;
        let relative = move |dx: f32, dy: f32| base + Offset::new(dx, dy);
        let (mut cubic, mut quad) = (None, None);
        match *node {
            Close => {
                data.push(PathCommand::Close);
                current = start;
            }
            RelativeMoveTo { dx, dy } | MoveTo { x: dx, y: dy } => {
                current = if matches!(node, MoveTo { .. }) {
                    Offset::new(dx, dy)
                } else {
                    relative(dx, dy)
                };
                start = current;
                data.push(PathCommand::MoveTo(current));
            }
            RelativeLineTo { dx, dy } => current = line(&mut data, relative(dx, dy)),
            LineTo { x, y } => current = line(&mut data, Offset::new(x, y)),
            RelativeHorizontalTo { dx } => current = line(&mut data, relative(dx, 0.0)),
            HorizontalTo { x } => current = line(&mut data, Offset::new(x, current.y)),
            RelativeVerticalTo { dy } => current = line(&mut data, relative(0.0, dy)),
            VerticalTo { y } => current = line(&mut data, Offset::new(current.x, y)),
            RelativeCurveTo { dx1, dy1, dx2, dy2, dx3, dy3 } => {
                let (first, second) = (relative(dx1, dy1), relative(dx2, dy2));
                current = relative(dx3, dy3);
                data.push(PathCommand::CubicTo(first, second, current));
                cubic = Some(second);
            }
            CurveTo { x1, y1, x2, y2, x3, y3 } => {
                let second = Offset::new(x2, y2);
                current = Offset::new(x3, y3);
                data.push(PathCommand::CubicTo(Offset::new(x1, y1), second, current));
                cubic = Some(second);
            }
            RelativeReflectiveCurveTo { dx1: x1, dy1: y1, dx2: x2, dy2: y2 }
            | ReflectiveCurveTo { x1, y1, x2, y2 } => {
                let absolute = matches!(node, ReflectiveCurveTo { .. });
                let point = |x, y| if absolute { Offset::new(x, y) } else { relative(x, y) };
                let first = last_cubic.map_or(current, |control| current + (current - control));
                let second = point(x1, y1);
                current = point(x2, y2);
                data.push(PathCommand::CubicTo(first, second, current));
                cubic = Some(second);
            }
            RelativeQuadTo { dx1: x1, dy1: y1, dx2: x2, dy2: y2 } | QuadTo { x1, y1, x2, y2 } => {
                let absolute = matches!(node, QuadTo { .. });
                let point = |x, y| if absolute { Offset::new(x, y) } else { relative(x, y) };
                let control = point(x1, y1);
                current = point(x2, y2);
                data.push(PathCommand::QuadTo(control, current));
                quad = Some(control);
            }
            RelativeReflectiveQuadTo { dx: x, dy: y } | ReflectiveQuadTo { x, y } => {
                let absolute = matches!(node, ReflectiveQuadTo { .. });
                let control = last_quad.map_or(current, |control| current + (current - control));
                current = if absolute { Offset::new(x, y) } else { relative(x, y) };
                data.push(PathCommand::QuadTo(control, current));
                quad = Some(control);
            }
            RelativeArcTo {
                horizontal_ellipse_radius: rx,
                vertical_ellipse_radius: ry,
                theta,
                is_more_than_half,
                is_positive_arc,
                arc_start_dx: x,
                arc_start_dy: y,
            }
            | ArcTo {
                horizontal_ellipse_radius: rx,
                vertical_ellipse_radius: ry,
                theta,
                is_more_than_half,
                is_positive_arc,
                arc_start_x: x,
                arc_start_y: y,
            } => {
                let end =
                    if matches!(node, ArcTo { .. }) { Offset::new(x, y) } else { relative(x, y) };
                crate::path::svg_arc(
                    &mut data,
                    current,
                    end,
                    rx,
                    ry,
                    theta,
                    is_more_than_half,
                    is_positive_arc,
                );
                current = end;
            }
        }
        last_cubic = cubic;
        last_quad = quad;
    }
    data
}

fn line(data: &mut PathData, to: Offset) -> Offset {
    data.push(PathCommand::LineTo(to));
    to
}

/// Builds path data, as Compose's `PathBuilder` (what `PathData { }` and a builder's `path { }`
/// give): each call adds a node.
#[derive(Default, Debug)]
pub struct PathBuilder {
    nodes: Vec<PathNode>,
}

macro_rules! builder_methods {
    ($($(#[$doc:meta])* $name:ident($($arg:ident),*) => $node:ident;)*) => {
        $(
            $(#[$doc])*
            pub fn $name(&mut self, $($arg: f32),*) -> &mut PathBuilder {
                self.nodes.push(PathNode::$node { $($arg),* });
                self
            }
        )*
    };
}

impl PathBuilder {
    pub fn new() -> PathBuilder {
        PathBuilder::default()
    }

    /// The nodes built, as `nodes`.
    pub fn nodes(&self) -> &[PathNode] {
        &self.nodes
    }

    pub fn close(&mut self) -> &mut PathBuilder {
        self.nodes.push(PathNode::Close);
        self
    }

    builder_methods! {
        move_to(x, y) => MoveTo;
        move_to_relative(dx, dy) => RelativeMoveTo;
        line_to(x, y) => LineTo;
        line_to_relative(dx, dy) => RelativeLineTo;
        horizontal_line_to(x) => HorizontalTo;
        horizontal_line_to_relative(dx) => RelativeHorizontalTo;
        vertical_line_to(y) => VerticalTo;
        vertical_line_to_relative(dy) => RelativeVerticalTo;
        curve_to(x1, y1, x2, y2, x3, y3) => CurveTo;
        curve_to_relative(dx1, dy1, dx2, dy2, dx3, dy3) => RelativeCurveTo;
        reflective_curve_to(x1, y1, x2, y2) => ReflectiveCurveTo;
        reflective_curve_to_relative(dx1, dy1, dx2, dy2) => RelativeReflectiveCurveTo;
        quad_to(x1, y1, x2, y2) => QuadTo;
        quad_to_relative(dx1, dy1, dx2, dy2) => RelativeQuadTo;
        reflective_quad_to(x, y) => ReflectiveQuadTo;
        reflective_quad_to_relative(dx, dy) => RelativeReflectiveQuadTo;
    }

    /// An elliptical arc to (`x`, `y`), as `arcTo`.
    #[allow(clippy::too_many_arguments)]
    pub fn arc_to(
        &mut self,
        horizontal_ellipse_radius: f32,
        vertical_ellipse_radius: f32,
        theta: f32,
        is_more_than_half: bool,
        is_positive_arc: bool,
        x: f32,
        y: f32,
    ) -> &mut PathBuilder {
        self.nodes.push(PathNode::ArcTo {
            horizontal_ellipse_radius,
            vertical_ellipse_radius,
            theta,
            is_more_than_half,
            is_positive_arc,
            arc_start_x: x,
            arc_start_y: y,
        });
        self
    }

    /// An elliptical arc to the current point moved by (`dx`, `dy`), as `arcToRelative`.
    #[allow(clippy::too_many_arguments)]
    pub fn arc_to_relative(
        &mut self,
        horizontal_ellipse_radius: f32,
        vertical_ellipse_radius: f32,
        theta: f32,
        is_more_than_half: bool,
        is_positive_arc: bool,
        dx: f32,
        dy: f32,
    ) -> &mut PathBuilder {
        self.nodes.push(PathNode::RelativeArcTo {
            horizontal_ellipse_radius,
            vertical_ellipse_radius,
            theta,
            is_more_than_half,
            is_positive_arc,
            arc_start_dx: dx,
            arc_start_dy: dy,
        });
        self
    }
}

/// The nodes `block` builds, as Compose's `PathData { … }`.
pub fn path_data(block: impl FnOnce(&mut PathBuilder)) -> Vec<PathNode> {
    let mut builder = PathBuilder::new();
    block(&mut builder);
    builder.nodes
}

/// A group of a vector, as Compose's `VectorGroup`: its children transformed (scaled and
/// rotated, in degrees, around its pivot, then moved) and clipped to `clip_path_data`. Its
/// defaults are Compose's (`DefaultGroupName`, `DefaultRotation`, …): no transform, no clip.
#[derive(Clone, PartialEq, Debug)]
pub struct VectorGroup {
    pub name: String,
    pub rotation: f32,
    pub pivot_x: f32,
    pub pivot_y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub translation_x: f32,
    pub translation_y: f32,
    pub clip_path_data: Vec<PathNode>,
    pub children: Vec<VectorNode>,
}

impl Default for VectorGroup {
    fn default() -> Self {
        VectorGroup {
            name: String::new(),
            rotation: 0.0,
            pivot_x: 0.0,
            pivot_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            translation_x: 0.0,
            translation_y: 0.0,
            clip_path_data: Vec::new(),
            children: Vec::new(),
        }
    }
}

/// A path of a vector, as Compose's `VectorPath`: filled and stroked by brushes. Its defaults
/// are Compose's (`DefaultPathName`, `DefaultStrokeLineWidth`, `DefaultStrokeLineMiter`, …):
/// neither filled nor stroked, opaque, a hairline with butt caps and miter joins, untrimmed.
#[derive(Clone, PartialEq, Debug)]
pub struct VectorPath {
    pub name: String,
    pub path_data: Vec<PathNode>,
    pub path_fill_type: PathFillType,
    pub fill: Option<Brush>,
    pub fill_alpha: f32,
    pub stroke: Option<Brush>,
    pub stroke_alpha: f32,
    pub stroke_line_width: f32,
    pub stroke_line_cap: StrokeCap,
    pub stroke_line_join: StrokeJoin,
    pub stroke_line_miter: f32,
    pub trim_path_start: f32,
    pub trim_path_end: f32,
    pub trim_path_offset: f32,
}

impl Default for VectorPath {
    fn default() -> Self {
        VectorPath {
            name: String::new(),
            path_data: Vec::new(),
            path_fill_type: PathFillType::NonZero,
            fill: None,
            fill_alpha: 1.0,
            stroke: None,
            stroke_alpha: 1.0,
            stroke_line_width: 0.0,
            stroke_line_cap: StrokeCap::Butt,
            stroke_line_join: StrokeJoin::Miter,
            stroke_line_miter: 4.0,
            trim_path_start: 0.0,
            trim_path_end: 1.0,
            trim_path_offset: 0.0,
        }
    }
}

/// A group's child, as Compose's `VectorNode`.
#[derive(Clone, PartialEq, Debug)]
pub enum VectorNode {
    Group(VectorGroup),
    Path(VectorPath),
}

/// What a vector draws, built once from its groups and paths: each path's contents made, so
/// every painter of the vector draws the same shared contents (as Compose's `GroupComponent`
/// and `PathComponent` hold built paths).
#[derive(Debug)]
enum RenderNode {
    Group(RenderGroup),
    Path(RenderPath),
}

#[derive(Debug, Default)]
struct RenderGroup {
    /// Pivot, scale, rotation and translation, if not the identity.
    transform: Option<(Offset, (f32, f32), f32, Offset)>,
    clip: Option<Arc<PathData>>,
    children: Vec<RenderNode>,
}

#[derive(Debug)]
struct RenderPath {
    path: Arc<PathData>,
    fill: Option<(Brush, f32)>,
    /// Brush, alpha, width, cap.
    stroke: Option<(Brush, f32, f32, StrokeCap)>,
}

/// `data` trimmed as `path` asks, as Compose's `PathComponent.updateRenderPath` (D1908): the
/// piece from the start to the end (fractions of the first contour's length, moved by the
/// offset and wrapped), two pieces when it wraps past the end. As Compose's, the trimmed path is
/// a new one: its fill type is the default.
fn trimmed(data: Arc<PathData>, path: &VectorPath) -> Arc<PathData> {
    let (start, end) = (path.trim_path_start, path.trim_path_end);
    if start == 0.0 && end == 1.0 {
        return data;
    }
    let mut measure = crate::path_measure::PathMeasure::new();
    measure.set_path(Some(&crate::path::Path::from_shared_data(data)), false);
    let length = measure.length();
    let start = ((start + path.trim_path_offset) % 1.0) * length;
    let end = ((end + path.trim_path_offset) % 1.0) * length;
    let render = crate::path::Path::new();
    if start > end {
        let piece = crate::path::Path::new();
        measure.get_segment(start, length, &piece, true);
        render.add_path(&piece, Offset::ZERO);
        piece.reset();
        measure.get_segment(0.0, end, &piece, true);
        render.add_path(&piece, Offset::ZERO);
    } else {
        measure.get_segment(start, end, &render, true);
    }
    render.data()
}

impl RenderGroup {
    fn of(group: &VectorGroup) -> RenderGroup {
        let identity = group.rotation == 0.0
            && group.scale_x == 1.0
            && group.scale_y == 1.0
            && group.translation_x == 0.0
            && group.translation_y == 0.0;
        let transform = (!identity).then_some((
            Offset::new(group.pivot_x, group.pivot_y),
            (group.scale_x, group.scale_y),
            group.rotation,
            Offset::new(group.translation_x, group.translation_y),
        ));
        let clip = (!group.clip_path_data.is_empty())
            .then(|| Arc::new(nodes_to_data(&group.clip_path_data, PathFillType::NonZero)));
        let children = group
            .children
            .iter()
            .map(|child| match child {
                VectorNode::Group(group) => RenderNode::Group(RenderGroup::of(group)),
                VectorNode::Path(path) => RenderNode::Path(RenderPath {
                    path: trimmed(
                        Arc::new(nodes_to_data(&path.path_data, path.path_fill_type)),
                        path,
                    ),
                    fill: path.fill.clone().map(|brush| (brush, path.fill_alpha)),
                    stroke: path.stroke.clone().map(|brush| {
                        (brush, path.stroke_alpha, path.stroke_line_width, path.stroke_line_cap)
                    }),
                }),
            })
            .collect();
        RenderGroup { transform, clip, children }
    }

    /// Its paths and their colors, if it's only solid fills, with no transform or clip down
    /// to them (as Material's icons).
    fn solid_fills(&self, into: &mut Vec<(Arc<PathData>, Color)>) -> bool {
        if self.transform.is_some() || self.clip.is_some() {
            return false;
        }
        self.children.iter().all(|child| match child {
            RenderNode::Group(group) => group.solid_fills(into),
            RenderNode::Path(RenderPath { path, fill, stroke: None }) => match fill {
                Some((Brush::Solid(color), alpha)) => {
                    let alpha = color.alpha() * alpha.clamp(0.0, 1.0);
                    into.push((Arc::clone(path), color.with_alpha(alpha)));
                    true
                }
                None => true,
                Some(_) => false,
            },
            RenderNode::Path(_) => false,
        })
    }

    /// Whether a group in it rotates.
    fn rotates(&self) -> bool {
        self.transform.is_some_and(|(_, _, rotation, _)| rotation != 0.0)
            || self.children.iter().any(|child| match child {
                RenderNode::Group(group) => group.rotates(),
                RenderNode::Path(_) => false,
            })
    }

    /// Draws it with its viewport scaled by `scale`, in the drawing's pixels: each path scaled
    /// by its `DrawPath` (Android draws a small one from a mask at that size, D190), each
    /// group's transform and clip moved to pixels. Exact for any group transform when the
    /// scale is uniform, and for any without rotation; a gradient's path is drawn under the
    /// scale, its positions in the viewport (`viewport`, its default extent).
    fn draw(&self, scope: &mut DrawScope<'_>, scale: (f32, f32), viewport: Size) {
        let scaled = |point: Offset| Offset::new(point.x * scale.0, point.y * scale.1);
        let saved = self.transform.is_some() || self.clip.is_some();
        if saved {
            scope.save();
        }
        if let Some((pivot, group_scale, rotation, translation)) = self.transform {
            scope.transform(scaled(pivot), group_scale, rotation, scaled(translation));
        }
        if let Some(clip) = &self.clip {
            let clip = if scale == (1.0, 1.0) {
                Arc::clone(clip)
            } else {
                let path = crate::path::Path::from_shared_data(Arc::clone(clip));
                path.transform(&crate::path::Matrix::scaling(scale.0, scale.1));
                path.data()
            };
            let offset = scope.origin;
            scope.list.push(DrawOp::ClipPath { path: clip, offset });
        }
        for child in &self.children {
            let RenderNode::Path(path) = child else {
                if let RenderNode::Group(group) = child {
                    group.draw(scope, scale, viewport);
                }
                continue;
            };
            let fill = path
                .fill
                .as_ref()
                .map(|(brush, alpha)| (brush, *alpha, PaintStyle::Fill, StrokeCap::Butt));
            let stroke = path.stroke.as_ref().map(|(brush, alpha, width, cap)| {
                (brush, *alpha, PaintStyle::Stroke(*width), *cap)
            });
            for (brush, alpha, style, cap) in fill.into_iter().chain(stroke) {
                if let Brush::Solid(_) = brush {
                    let mut paint = Paint::fill(brush.color(alpha));
                    paint.style = style;
                    paint.cap = cap;
                    scope.draw_path_scaled(&path.path, Offset::ZERO, scale, paint);
                    continue;
                }
                scope.save();
                scope.transform(Offset::ZERO, scale, 0.0, Offset::ZERO);
                let drawn = scope.size;
                scope.size = viewport;
                let mut paint = scope.brush_paint(brush, alpha, style);
                paint.cap = cap;
                scope.draw_path(&path.path, paint);
                scope.size = drawn;
                scope.restore();
            }
        }
        if saved {
            scope.restore();
        }
    }
}

struct ImageVectorData {
    name: String,
    default_width: Dp,
    default_height: Dp,
    viewport_width: f32,
    viewport_height: f32,
    root: VectorGroup,
    tint_color: Color,
    tint_blend_mode: BlendMode,
    auto_mirror: bool,
    render: RenderGroup,
    solid_fills: Option<Vec<(Arc<PathData>, Color)>>,
}

/// A vector image, as Compose's `ImageVector`: a tree of groups and paths in a viewport, drawn
/// at a default size, tinted by its `tint_color` with `tint_blend_mode` if specified, mirrored
/// right to left if `auto_mirror`. Built with [`ImageVector::builder`]; immutable and shared (a
/// clone copies nothing), its paths made once for every painter drawing it.
#[derive(Clone)]
pub struct ImageVector(Rc<ImageVectorData>);

impl ImageVector {
    /// A builder of a vector of a default size and viewport, as `ImageVector.Builder`.
    pub fn builder(
        default_width: Dp,
        default_height: Dp,
        viewport_width: f32,
        viewport_height: f32,
    ) -> ImageVectorBuilder {
        ImageVectorBuilder {
            name: String::new(),
            default_width,
            default_height,
            viewport_width,
            viewport_height,
            tint_color: Color::UNSPECIFIED,
            tint_blend_mode: BlendMode::SrcIn,
            auto_mirror: false,
            stack: vec![VectorGroup::default()],
            built: false,
        }
    }

    /// A 24 dp icon of one black path in a 24 × 24 viewport, from SVG path data, as Material's
    /// icons (`materialIcon { materialPath { … } }`).
    pub fn icon(name: &str, data: &str) -> ImageVector {
        ImageVector::builder(Dp(24.0), Dp(24.0), 24.0, 24.0)
            .name(name)
            .add_path(VectorPath {
                path_data: add_path_nodes(data),
                fill: Some(Brush::solid(Color::BLACK)),
                ..VectorPath::default()
            })
            .build()
    }

    pub fn name(&self) -> &str {
        &self.0.name
    }
    pub fn default_width(&self) -> Dp {
        self.0.default_width
    }
    pub fn default_height(&self) -> Dp {
        self.0.default_height
    }
    pub fn viewport_width(&self) -> f32 {
        self.0.viewport_width
    }
    pub fn viewport_height(&self) -> f32 {
        self.0.viewport_height
    }
    /// Its root group, as `root`.
    pub fn root(&self) -> &VectorGroup {
        &self.0.root
    }
    pub fn tint_color(&self) -> Color {
        self.0.tint_color
    }
    pub fn tint_blend_mode(&self) -> BlendMode {
        self.0.tint_blend_mode
    }
    pub fn auto_mirror(&self) -> bool {
        self.0.auto_mirror
    }

    /// Its paths' contents and colors, if it's only filled with solid colors, with no group
    /// transform or clip (as Material's icons): what an icon can draw directly, each path
    /// scaled, without a painter.
    pub fn solid_fills(&self) -> Option<&[(Arc<PathData>, Color)]> {
        self.0.solid_fills.as_deref()
    }

    /// Whether `other` is this vector, not only an equal one.
    pub fn ptr_eq(&self, other: &ImageVector) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// Equal when what they hold is, as Compose's `ImageVector.equals`.
impl PartialEq for ImageVector {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (&self.0, &other.0);
        self.ptr_eq(other)
            || (a.name == b.name
                && a.default_width == b.default_width
                && a.default_height == b.default_height
                && a.viewport_width == b.viewport_width
                && a.viewport_height == b.viewport_height
                && a.root == b.root
                && a.tint_color == b.tint_color
                && a.tint_blend_mode == b.tint_blend_mode
                && a.auto_mirror == b.auto_mirror)
    }
}

impl std::fmt::Debug for ImageVector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "ImageVector({:?})", self.0.name)
    }
}

impl compose_runtime::ComposeArg for ImageVector {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// Builds an [`ImageVector`], as Compose's `ImageVector.Builder`: groups and paths added where
/// the current group is, `add_group` entering a group and `clear_group` leaving it.
#[derive(Debug)]
pub struct ImageVectorBuilder {
    name: String,
    default_width: Dp,
    default_height: Dp,
    viewport_width: f32,
    viewport_height: f32,
    tint_color: Color,
    tint_blend_mode: BlendMode,
    auto_mirror: bool,
    /// The root, then each group entered.
    stack: Vec<VectorGroup>,
    built: bool,
}

impl ImageVectorBuilder {
    pub fn name(&mut self, name: impl Into<String>) -> &mut Self {
        self.name = name.into();
        self
    }
    pub fn tint_color(&mut self, tint_color: Color) -> &mut Self {
        self.tint_color = tint_color;
        self
    }
    pub fn tint_blend_mode(&mut self, tint_blend_mode: BlendMode) -> &mut Self {
        self.tint_blend_mode = tint_blend_mode;
        self
    }
    pub fn auto_mirror(&mut self, auto_mirror: bool) -> &mut Self {
        self.auto_mirror = auto_mirror;
        self
    }

    fn check_not_built(&self) {
        assert!(!self.built, "compose-ui: an ImageVector builder is single use");
    }

    /// Enters `group` (its children are what's added until `clear_group`), as `addGroup`.
    pub fn add_group(&mut self, group: VectorGroup) -> &mut Self {
        self.check_not_built();
        self.stack.push(group);
        self
    }

    /// Adds `path` to the current group, as `addPath`.
    pub fn add_path(&mut self, path: VectorPath) -> &mut Self {
        self.check_not_built();
        self.stack.last_mut().expect("the root").children.push(VectorNode::Path(path));
        self
    }

    /// Leaves the current group, as `clearGroup` (the root stays).
    pub fn clear_group(&mut self) -> &mut Self {
        self.check_not_built();
        if self.stack.len() > 1 {
            let group = self.stack.pop().expect("a group");
            self.stack.last_mut().expect("the root").children.push(VectorNode::Group(group));
        }
        self
    }

    /// `group` with what `block` adds in it, as the builder's `group { … }`.
    pub fn group(&mut self, group: VectorGroup, block: impl FnOnce(&mut Self)) -> &mut Self {
        self.add_group(group);
        block(self);
        self.clear_group()
    }

    /// `path` with the path data `block` builds, as the builder's `path { … }`.
    pub fn path(&mut self, path: VectorPath, block: impl FnOnce(&mut PathBuilder)) -> &mut Self {
        self.add_path(VectorPath { path_data: path_data(block), ..path })
    }

    /// The vector, every group left, as `build`.
    ///
    /// # Panics
    ///
    /// If it was built already, as Compose's builder is single use.
    pub fn build(&mut self) -> ImageVector {
        self.check_not_built();
        while self.stack.len() > 1 {
            self.clear_group();
        }
        self.built = true;
        let root = self.stack.pop().expect("the root");
        let render = RenderGroup::of(&root);
        let mut fills = Vec::new();
        let solid_fills = render.solid_fills(&mut fills).then_some(fills);
        ImageVector(Rc::new(ImageVectorData {
            name: std::mem::take(&mut self.name),
            default_width: self.default_width,
            default_height: self.default_height,
            viewport_width: self.viewport_width,
            viewport_height: self.viewport_height,
            root,
            tint_color: self.tint_color,
            tint_blend_mode: self.tint_blend_mode,
            auto_mirror: self.auto_mirror,
            render,
            solid_fills,
        }))
    }
}

/// What a vector painter last recorded, as Compose's `DrawCache`: its drawing at a size (and
/// whether mirrored), in its own coordinates, copied into each drawing at that size instead of
/// walking the vector again.
#[derive(Default)]
struct DrawCache {
    key: Option<(Size, bool)>,
    recording: DisplayList,
}

/// A painter drawing an [`ImageVector`], as Compose's `VectorPainter`: its intrinsic size the
/// vector's default size, its viewport scaled to the size it's drawn at, mirrored right to left
/// if the vector asks, through the color filter given (or the vector's tint), at the alpha
/// given, in a layer when either applies. Its drawing at a size is recorded once and copied
/// after ([`remember_vector_painter`] keeps one per vector and density).
pub struct VectorPainter {
    image: ImageVector,
    size: Size,
    alpha: Cell<f32>,
    color_filter: Cell<Option<ColorFilter>>,
    cache: RefCell<DrawCache>,
    /// How many times it recorded its drawing: for tests.
    recordings: Cell<usize>,
}

impl VectorPainter {
    /// A painter of `image` at `density`.
    pub fn new(image: ImageVector, density: Density) -> VectorPainter {
        let size =
            Size::new(density.to_px(image.default_width()), density.to_px(image.default_height()));
        VectorPainter {
            image,
            size,
            alpha: Cell::new(1.0),
            color_filter: Cell::new(None),
            cache: RefCell::new(DrawCache::default()),
            recordings: Cell::new(0),
        }
    }

    pub fn image(&self) -> &ImageVector {
        &self.image
    }

    /// How many times it recorded its drawing (once per size it's drawn at, unless the size
    /// changes back and forth), as the tests of Compose's cache count.
    pub fn recordings(&self) -> usize {
        self.recordings.get()
    }

    /// Records the vector at `size` into `scope` (at its origin 0): its viewport scaled.
    fn record(&self, scope: &mut DrawScope<'_>, size: Size, mirrored: bool) {
        let data = &self.image.0;
        let scale = (size.width / data.viewport_width, size.height / data.viewport_height);
        if mirrored {
            scope.save();
            let center = Offset::new(size.width / 2.0, size.height / 2.0);
            scope.transform(center, (-1.0, 1.0), 0.0, Offset::ZERO);
        }
        let viewport = Size::new(data.viewport_width, data.viewport_height);
        if let Some(fills) = &data.solid_fills {
            // Each path scaled itself, as Android draws a small one from a mask (D190).
            for (path, color) in fills {
                scope.draw_path_scaled(path, Offset::ZERO, scale, Paint::fill(*color));
            }
        } else if scale.0 == scale.1 || !data.render.rotates() {
            data.render.draw(scope, scale, viewport);
        } else {
            // A rotation under a stretch: the whole viewport scaled.
            scope.save();
            scope.transform(Offset::ZERO, scale, 0.0, Offset::ZERO);
            data.render.draw(scope, (1.0, 1.0), viewport);
            scope.restore();
        }
        if mirrored {
            scope.restore();
        }
    }
}

impl CustomPainter for VectorPainter {
    fn intrinsic_size(&self) -> Size {
        self.size
    }

    fn on_draw(&self, scope: &mut DrawScope<'_>) {
        let size = scope.size();
        let data = &self.image.0;
        if !(size.width > 0.0 && size.height > 0.0)
            || data.viewport_width <= 0.0
            || data.viewport_height <= 0.0
        {
            return;
        }
        let intrinsic = data
            .tint_color
            .is_specified()
            .then(|| ColorFilter::tint_with(data.tint_color, data.tint_blend_mode));
        let filter = self.color_filter.get().or(intrinsic);
        let alpha = self.alpha.get();
        let layered = alpha < 1.0 || filter.is_some();
        if layered {
            let bounds = Rect::new(0.0, 0.0, size.width, size.height);
            scope.save_layer(bounds, alpha, filter.as_ref(), BlendMode::SrcOver);
        }
        let mirrored = data.auto_mirror && scope.layout_direction() == LayoutDirection::Rtl;
        let mut cache = self.cache.borrow_mut();
        if cache.key != Some((size, mirrored)) {
            cache.recording.clear();
            let mut recorder = DrawScope {
                list: &mut cache.recording,
                origin: Offset::ZERO,
                size,
                density: scope.density,
                layout_direction: scope.layout_direction,
                locals: scope.locals,
            };
            self.record(&mut recorder, size, mirrored);
            cache.key = Some((size, mirrored));
            self.recordings.set(self.recordings.get() + 1);
        }
        replay(scope, &cache.recording);
        if layered {
            scope.restore();
        }
    }

    fn apply_alpha(&self, alpha: f32) -> bool {
        self.alpha.set(alpha);
        true
    }

    fn apply_color_filter(&self, color_filter: Option<&ColorFilter>) -> bool {
        self.color_filter.set(color_filter.copied());
        true
    }
}

/// Copies `recording` (made at origin 0) into `scope` at its origin, its shaders and filters
/// with it.
fn replay(scope: &mut DrawScope<'_>, recording: &DisplayList) {
    let list = &mut *scope.list;
    let bases = (list.shaders.len() as u32, list.color_filters.len() as u32);
    list.shaders.extend_from_slice(&recording.shaders);
    list.color_filters.extend_from_slice(&recording.color_filters);
    let origin = scope.origin;
    let moved = origin != Offset::ZERO;
    if moved {
        list.push(DrawOp::Translate(origin));
    }
    crate::display_list::extend_shifted(&mut list.ops, &recording.ops, bases);
    if moved {
        list.push(DrawOp::Translate(Offset::new(0.0 - origin.x, 0.0 - origin.y)));
    }
}

/// A painter of `image` at the current density, kept while they stay, as Compose's
/// `rememberVectorPainter(image)`. Named `remember_…` as it takes a slot.
pub fn remember_vector_painter(image: &ImageVector) -> Painter {
    let density = crate::host::LOCAL_DENSITY.current();
    let image = image.clone();
    compose_runtime::remember_keyed((image.clone(), density.density.to_bits()), move || {
        Painter::new(VectorPainter::new(image, density))
    })
}
