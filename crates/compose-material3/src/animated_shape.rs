//! A shape whose corners follow another, as Compose's `rememberAnimatedShape`: one shape object,
//! so what holds it (a background, a clip, a ripple's ring) stays put while its corners move, read
//! where its outline is.

use std::cell::RefCell;
use std::rc::Rc;

use compose_animation::{Animatable, AnimationSpec};
use compose_runtime::LaunchedEffect;
use compose_ui::{CustomShape, Density, LayoutDirection, Outline, Rect, Shape, Size};

/// `start` to `target`, `progress` of the way, as `AnimatedShapeState`.
struct AnimatedShapeState {
    start: RefCell<Shape>,
    target: RefCell<Shape>,
    progress: Animatable<f32>,
    spec: AnimationSpec,
}

/// The corners of `outline`, in pixels, if it has corners to move.
fn corners(outline: &Outline) -> Option<(Rect, [f32; 4])> {
    match outline {
        Outline::Rectangle(rect) => Some((*rect, [0.0; 4])),
        Outline::Rounded(rect, radii) => Some((*rect, *radii)),
        Outline::Generic(..) => None,
    }
}

/// The outline `fraction` of the way between those of `start` and `target`: their corners, in
/// pixels; a generic outline, which has none, snaps.
fn morphed(
    start: &Shape,
    target: &Shape,
    fraction: f32,
    size: Size,
    direction: LayoutDirection,
    density: &Density,
) -> Outline {
    let to = target.outline_in(size, density, direction);
    let from = start.outline_in(size, density, direction);
    let (Some((rect, from)), Some((_, end))) = (corners(&from), corners(&to)) else { return to };
    let radii = std::array::from_fn(|index| from[index] + (end[index] - from[index]) * fraction);
    if radii == [0.0; 4] { Outline::Rectangle(rect) } else { Outline::Rounded(rect, radii) }
}

/// A frozen point between two shapes, where a new target found the last animation.
struct FrozenShape {
    start: Shape,
    target: Shape,
    fraction: f32,
}

impl CustomShape for FrozenShape {
    fn create_outline(
        &self,
        size: Size,
        layout_direction: LayoutDirection,
        density: &Density,
    ) -> Outline {
        morphed(&self.start, &self.target, self.fraction, size, layout_direction, density)
    }

    fn same(&self, other: &dyn CustomShape) -> bool {
        other.downcast_ref::<FrozenShape>().is_some_and(|other| {
            self.start == other.start
                && self.target == other.target
                && self.fraction == other.fraction
        })
    }
}

/// The shape handed out: the morph as it is now, the read of its progress wherever the outline is
/// made (a draw, a layer) redrawing that.
struct AnimatedShape(Rc<AnimatedShapeState>);

impl CustomShape for AnimatedShape {
    fn create_outline(
        &self,
        size: Size,
        layout_direction: LayoutDirection,
        density: &Density,
    ) -> Outline {
        let state = &self.0;
        let fraction = state.progress.value();
        morphed(
            &state.start.borrow(),
            &state.target.borrow(),
            fraction,
            size,
            layout_direction,
            density,
        )
    }

    fn same(&self, other: &dyn CustomShape) -> bool {
        other.downcast_ref::<AnimatedShape>().is_some_and(|other| Rc::ptr_eq(&self.0, &other.0))
    }
}

impl AnimatedShapeState {
    /// As `animateToShape`.
    async fn animate_to(&self, new_target: Shape) {
        if *self.target.borrow() == new_target {
            return;
        }
        if *self.start.borrow() == new_target {
            // Reversed before the animation finished: its progress flipped, its velocity too.
            let old_target = self.target.replace(new_target);
            *self.start.borrow_mut() = old_target;
            let (progress, velocity) = (self.progress.value(), self.progress.velocity());
            self.progress.snap_to(1.0 - progress);
            self.progress.animate_to_with_velocity(1.0, self.spec.clone(), -velocity).await;
        } else {
            // A new target: from what shows now, frozen, and only then if it's between two.
            let progress = self.progress.value();
            let frozen = if progress == 1.0 {
                self.target.borrow().clone()
            } else if progress == 0.0 {
                self.start.borrow().clone()
            } else {
                Shape::custom(FrozenShape {
                    start: self.start.borrow().clone(),
                    target: self.target.borrow().clone(),
                    fraction: progress,
                })
            };
            *self.start.borrow_mut() = frozen;
            *self.target.borrow_mut() = new_target;
            self.progress.snap_to(0.0);
            self.progress.animate_to(1.0, self.spec.clone()).await;
        }
    }
}

/// A shape morphing to `current` on `spec` whenever `current` changes, as
/// `rememberAnimatedShape(currentShape, animationSpec)`: the same shape object until `spec` changes.
pub(crate) fn remember_animated_shape(current: Shape, spec: AnimationSpec) -> Shape {
    let state = compose_runtime::remember_keyed(spec.clone(), {
        let current = current.clone();
        move || {
            Rc::new(AnimatedShapeState {
                start: RefCell::new(current.clone()),
                target: RefCell::new(current),
                progress: Animatable::new(1.0),
                spec,
            })
        }
    });
    LaunchedEffect((current.clone(), state.clone()), {
        let state = state.clone();
        async move { state.animate_to(current).await }
    });
    Shape::custom(AnimatedShape(state))
}
