//! Paths, as Compose's `Path`: lines, curves and closes, filled or stroked; parsed from SVG path
//! data (`PathParser`), which is how vector icons ship.

use std::sync::Arc;

use crate::unit::Offset;

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

/// A path, as Compose's `Path`: built with `move_to`, `line_to`, … or parsed from SVG path data.
/// Shared (`Arc`) by the display list, so drawing it each frame copies nothing.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Path {
    commands: Vec<PathCommand>,
    pub fill_type: PathFillType,
}

impl Path {
    pub fn new() -> Path {
        Path::default()
    }

    pub fn commands(&self) -> &[PathCommand] {
        &self.commands
    }

    pub fn move_to(&mut self, x: f32, y: f32) {
        self.commands.push(PathCommand::MoveTo(Offset::new(x, y)));
    }

    pub fn line_to(&mut self, x: f32, y: f32) {
        self.commands.push(PathCommand::LineTo(Offset::new(x, y)));
    }

    pub fn quadratic_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.commands.push(PathCommand::QuadTo(Offset::new(x1, y1), Offset::new(x2, y2)));
    }

    pub fn cubic_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) {
        self.commands.push(PathCommand::CubicTo(
            Offset::new(x1, y1),
            Offset::new(x2, y2),
            Offset::new(x3, y3),
        ));
    }

    pub fn close(&mut self) {
        self.commands.push(PathCommand::Close);
    }

    /// Parses SVG path data (`M`, `L`, `H`, `V`, `C`, `S`, `Q`, `T`, `A`, `Z`, absolute and
    /// relative, numbers separated by spaces, commas or signs), as Compose's `PathParser`.
    pub fn from_svg(data: &str) -> Result<Path, PathError> {
        Parser { input: data.as_bytes(), position: 0 }.parse()
    }
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

    fn parse(mut self) -> Result<Path, PathError> {
        let mut path = Path::new();
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
                    path.commands.push(PathCommand::MoveTo(current));
                    // Further pairs are lines.
                    command = Some(if relative { b'l' } else { b'L' });
                }
                b'L' => {
                    current = point(&mut self)?;
                    path.commands.push(PathCommand::LineTo(current));
                }
                b'H' => {
                    let x = self.number()?;
                    current = Offset::new(if relative { current.x + x } else { x }, current.y);
                    path.commands.push(PathCommand::LineTo(current));
                }
                b'V' => {
                    let y = self.number()?;
                    current = Offset::new(current.x, if relative { current.y + y } else { y });
                    path.commands.push(PathCommand::LineTo(current));
                }
                b'C' => {
                    let first = point(&mut self)?;
                    let second = point(&mut self)?;
                    current = point(&mut self)?;
                    path.commands.push(PathCommand::CubicTo(first, second, current));
                    next_cubic = Some(second);
                }
                b'S' => {
                    let first = match last_cubic {
                        Some(control) => current + (current - control),
                        None => current,
                    };
                    let second = point(&mut self)?;
                    current = point(&mut self)?;
                    path.commands.push(PathCommand::CubicTo(first, second, current));
                    next_cubic = Some(second);
                }
                b'Q' => {
                    let control = point(&mut self)?;
                    current = point(&mut self)?;
                    path.commands.push(PathCommand::QuadTo(control, current));
                    next_quad = Some(control);
                }
                b'T' => {
                    let control = match last_quad {
                        Some(control) => current + (current - control),
                        None => current,
                    };
                    current = point(&mut self)?;
                    path.commands.push(PathCommand::QuadTo(control, current));
                    next_quad = Some(control);
                }
                b'A' => {
                    let radius_x = self.number()?;
                    let radius_y = self.number()?;
                    let rotation = self.number()?;
                    let large_arc = self.flag()?;
                    let sweep = self.flag()?;
                    let end = point(&mut self)?;
                    arc_to(&mut path, current, end, radius_x, radius_y, rotation, large_arc, sweep);
                    current = end;
                }
                b'Z' => {
                    path.commands.push(PathCommand::Close);
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
fn arc_to(
    path: &mut Path,
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
        path.commands.push(PathCommand::LineTo(to));
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
        path.commands.push(PathCommand::CubicTo(
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
    /// Each path with its fill (`None`: black, as Material's icons; tinted by `Icon`). Shared: a
    /// clone copies nothing.
    pub paths: Arc<[(Arc<Path>, Option<crate::graphics::Color>)]>,
}

impl ImageVector {
    /// A 24 dp icon of one path in a 24 × 24 viewport, from SVG path data, as Material's icons.
    pub fn icon(name: &'static str, data: &str) -> ImageVector {
        let path = Path::from_svg(data).unwrap_or_else(|error| panic!("icon {name}: {error}"));
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
        assert_eq!(path.commands()[0], PathCommand::MoveTo(Offset::new(19.0, 13.0)));
        assert_eq!(path.commands()[1], PathCommand::LineTo(Offset::new(13.0, 13.0)));
        assert_eq!(path.commands()[2], PathCommand::LineTo(Offset::new(13.0, 19.0)));
        assert_eq!(*path.commands().last().unwrap(), PathCommand::Close);
        assert_eq!(path.commands().len(), 14);
    }

    #[test]
    fn implicit_repeats_and_signs_separate_numbers() {
        let path = Path::from_svg("M9,16.17L4.83,12l-1.42,1.41L9,19 21,7l-1.41-1.41z").unwrap();
        assert_eq!(path.commands()[4], PathCommand::LineTo(Offset::new(21.0, 7.0)));
        assert_eq!(path.commands()[5], PathCommand::LineTo(Offset::new(19.59, 5.59)));
    }

    #[test]
    fn arcs_become_curves_ending_at_their_end() {
        let path = Path::from_svg("M2,12a10,10 0 1,0 20,0a10,10 0 1,0 -20,0z").unwrap();
        let ends: Vec<Offset> = path
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
}
