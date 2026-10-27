//! Borders, as Compose's `Modifier.border`, `BorderStroke` and `BorderRenderer`.

use crate::brush::Brush;
use crate::draw::DrawScope;
use crate::graphics::{Color, Outline, PaintStyle, Shape};
use crate::modifier::{DrawModifierNode, Modifier, ModifierNode};
use crate::path::{CornerRadius, Path, PathOperation, RoundRect};
use crate::unit::{Dp, Rect};

/// A border's width and brush, as Compose's `BorderStroke`.
#[derive(Clone, PartialEq, Debug)]
pub struct BorderStroke {
    pub width: Dp,
    pub brush: Brush,
}

impl BorderStroke {
    /// A border of `width` painted with `brush` (a `Color` for a solid one), as
    /// `BorderStroke(width, brush)` and `BorderStroke(width, color)`.
    pub fn new(width: Dp, brush: impl Into<Brush>) -> BorderStroke {
        BorderStroke { width, brush: brush.into() }
    }
}

impl compose_runtime::ComposeArg for BorderStroke {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// Draws borders inside an outline, as Compose's `BorderRenderer`: the border grows inwards
/// from the outline's bounds as its width grows.
#[derive(Clone, Copy, Default, Debug)]
pub struct BorderRenderer;

impl BorderRenderer {
    pub fn new() -> BorderRenderer {
        BorderRenderer
    }

    /// Draws a border of `width` painted with `brush` inside `outline`, as `drawBorder`: in the
    /// outline's bounds (not the scope's size). `Dp::HAIRLINE` (0) is 1 pixel; an unspecified or
    /// negative width draws nothing; a fractional width rounds up to whole pixels; a stroke more
    /// than half the shorter side fills the outline.
    pub fn draw_border(
        &self,
        scope: &mut DrawScope<'_>,
        width: Dp,
        brush: &Brush,
        outline: &Outline,
    ) {
        let width_px = if width == Dp::HAIRLINE {
            1.0
        } else if !width.is_specified() {
            0.0
        } else {
            scope.density().to_px(width).ceil()
        };
        if width_px <= 0.0 || width_px.is_nan() {
            return;
        }
        let bounds = outline.bounds();
        let (bounds_width, bounds_height) =
            (bounds.right - bounds.left, bounds.bottom - bounds.top);
        if bounds_width <= 0.0 || bounds_height <= 0.0 {
            return;
        }
        let shorter = bounds_width.min(bounds_height);
        let stroke = width_px.ceil().min((shorter / 2.0).ceil()).max(0.0);
        let half = stroke / 2.0;
        // The stroke is larger than the drawing area: the whole shape is filled.
        let fill = stroke * 2.0 > shorter;
        let fill_paint =
            |scope: &mut DrawScope<'_>| scope.brush_paint(brush, 1.0, PaintStyle::Fill);
        match outline {
            Outline::Rectangle(_) => {
                if fill {
                    let paint = fill_paint(scope);
                    scope.draw_rect(bounds, paint);
                } else {
                    let paint = scope.brush_paint(brush, 1.0, PaintStyle::Stroke(stroke));
                    scope.draw_rect(inset(bounds, half), paint);
                }
            }
            Outline::Rounded(rect, radii) => {
                let simple = radii.iter().all(|radius| *radius == radii[0]);
                if simple && fill {
                    let paint = fill_paint(scope);
                    scope.draw_round_rect(*rect, *radii, paint);
                } else if simple && radii[0] < half {
                    // The stroke's inside would be square: the rounded rectangle less the
                    // rectangle inside the stroke, as Compose's difference clip.
                    let path = Path::new();
                    path.add_round_rect(round_rect(*rect, *radii));
                    let inner = Path::new();
                    inner.add_rect(inset(*rect, stroke));
                    let border = Path::combine(PathOperation::Difference, &path, &inner);
                    let paint = fill_paint(scope);
                    scope.draw_path(&border, paint);
                } else if simple {
                    let paint = scope.brush_paint(brush, 1.0, PaintStyle::Stroke(stroke));
                    let shrunk = radii.map(|radius| (radius - half).max(0.0));
                    scope.draw_round_rect(inset(*rect, half), shrunk, paint);
                } else {
                    // Corners that differ: the rounded rectangle less the one inside the stroke.
                    let path = Path::new();
                    path.add_round_rect(round_rect(*rect, *radii));
                    let border = if fill {
                        path
                    } else {
                        let inner = Path::new();
                        let shrunk = radii.map(|radius| (radius - stroke).max(0.0));
                        inner.add_round_rect(round_rect(inset(*rect, stroke), shrunk));
                        Path::combine(PathOperation::Difference, &path, &inner)
                    };
                    let paint = fill_paint(scope);
                    scope.draw_path(&border, paint);
                }
            }
            Outline::Generic(path, offset) => {
                let offset = *offset;
                if fill {
                    let paint = fill_paint(scope);
                    scope.translate(offset.x, offset.y, |scope| scope.draw_path(path, paint));
                } else {
                    // A stroke twice as wide, centered on the path, inside it only: what
                    // Compose's draws and then clears outside the path.
                    let paint = scope.brush_paint(brush, 1.0, PaintStyle::Stroke(stroke * 2.0));
                    scope.translate(offset.x, offset.y, |scope| {
                        scope.clip_path(path, |scope| scope.draw_path(path, paint))
                    });
                }
            }
        }
    }
}

fn inset(rect: Rect, by: f32) -> Rect {
    Rect::new(rect.left + by, rect.top + by, rect.right - by, rect.bottom - by)
}

/// `rect` with `radii` (top left, top right, bottom right, bottom left).
fn round_rect(rect: Rect, radii: [f32; 4]) -> RoundRect {
    let corner = |radius: f32| CornerRadius::new(radius, radius);
    RoundRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
        top_left_corner_radius: corner(radii[0]),
        top_right_corner_radius: corner(radii[1]),
        bottom_right_corner_radius: corner(radii[2]),
        bottom_left_corner_radius: corner(radii[3]),
    }
}

/// A border painted with a gradient: a custom element, so the built-in elements stay small.
struct BorderBrushElement(BorderNode);

impl crate::modifier::ModifierNodeElement for BorderBrushElement {
    type Node = BorderNode;
    fn create(&self) -> BorderNode {
        self.0.clone()
    }
    fn update(&self, node: &mut BorderNode) {
        *node = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        other.0 == self.0
    }
    fn name(&self) -> &'static str {
        "border"
    }
}

/// `border`'s node: its width, brush and shape.
#[derive(Clone, PartialEq)]
pub(crate) struct BorderNode {
    pub(crate) width: Dp,
    pub(crate) brush: Brush,
    pub(crate) shape: Shape,
}

impl ModifierNode for BorderNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl DrawModifierNode for BorderNode {
    /// Over the content, as Compose's border.
    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        if let Brush::Solid(color) = self.brush
            && !color.is_specified()
        {
            return;
        }
        let outline =
            self.shape.outline_in(scope.size(), &scope.density(), scope.layout_direction());
        BorderRenderer.draw_border(scope, self.width, &self.brush, &outline);
    }
}

impl Modifier {
    /// A border of `width` and `color` around the bounds, as `Modifier.border(width, color)`.
    pub fn border(self, width: Dp, color: Color) -> Modifier {
        self.border_with(width, color, Shape::Rectangle)
    }

    /// A border of `width`, painted with `brush` (a `Color` for a solid one), inside `shape`, as
    /// `Modifier.border(width, color, shape)` and `border(width, brush, shape)`.
    pub fn border_with(self, width: Dp, brush: impl Into<Brush>, shape: Shape) -> Modifier {
        match brush.into() {
            Brush::Solid(color) => {
                self.then_element(crate::modifier::Element::Border(width, color, shape))
            }
            brush => self.then_custom(BorderBrushElement(BorderNode { width, brush, shape })),
        }
    }

    /// `border` from a `BorderStroke`, inside `shape`, as `Modifier.border(border, shape)`.
    pub fn border_stroke(self, border: BorderStroke, shape: Shape) -> Modifier {
        self.border_with(border.width, border.brush, shape)
    }
}
