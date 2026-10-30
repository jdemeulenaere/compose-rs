//! A shape whose corners follow another, as Compose's `rememberAnimatedShape`: one shape object,
//! so what holds it (a background, a clip, a ripple's ring) stays put while its corners move, read
//! where its outline is.

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;

use compose_animation::{Animatable, AnimationSpec};
use compose_runtime::{LaunchedEffect, OwnedState, mutable_state_of};
use compose_ui::{CustomShape, Density, LayoutDirection, Outline, Shape, Size};

/// `start` to `target`, `progress` of the way, as `AnimatedShapeState`.
struct AnimatedShapeState {
    start: RefCell<Shape>,
    target: RefCell<Shape>,
    /// Made with the first new target: each item of a lazy list holds one of these, and most never
    /// change. `started` is what an outline made before that subscribes to.
    progress: OnceCell<Animatable<f32>>,
    started: OwnedState<bool>,
    spec: AnimationSpec,
    /// A target other than the first has come: only then is there an effect to run it.
    moved: Cell<bool>,
}

/// The shape `shape` stands for: a frozen point as the shape it was, in pixels for `size`.
fn resolved(shape: &Shape, size: Size, density: &Density) -> Shape {
    match shape {
        Shape::Custom(custom) => match custom.downcast_ref::<FrozenShape>() {
            Some(frozen) => morphed(&frozen.start, &frozen.target, frozen.fraction, size, density),
            None => shape.clone(),
        },
        _ => shape.clone(),
    }
}

/// `fraction` of the way between `start` and `target`, as `Interpolatable.lerp`: corner by corner
/// in a family (`RoundedCornerShape.lerp`, `CutCornerShape.lerp`, a rectangle being any family's
/// zero corners), or the nearer of the two between families. A spring going past its ends can
/// take corners below zero: they stop at zero, rather than fail as Compose's do.
fn morphed(start: &Shape, target: &Shape, fraction: f32, size: Size, density: &Density) -> Shape {
    if fraction == 1.0 {
        return target.clone();
    }
    let start = resolved(start, size, density);
    let same_family = std::mem::discriminant(&start) == std::mem::discriminant(target);
    let family = match (&start, target) {
        (Shape::Rectangle, Shape::Rectangle) => None,
        (Shape::Rectangle, shape) | (shape, Shape::Rectangle) => Some(shape),
        (shape, _) => same_family.then_some(shape),
    };
    let Some(family) = family.filter(|shape| shape.corners().is_some()) else {
        return if fraction < 0.5 { start } else { target.clone() };
    };
    let in_pixels = |shape: &Shape| {
        shape
            .corners()
            .unwrap_or([compose_ui::CornerSize::ZERO; 4])
            .map(|it| it.to_px(size, density))
    };
    let (from, to) = (in_pixels(&start), in_pixels(target));
    let corner = |index: usize| {
        compose_ui::CornerSize::Px((from[index] + (to[index] - from[index]) * fraction).max(0.0))
    };
    family.copy((corner(0), corner(1), corner(2), corner(3)))
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
        morphed(&self.start, &self.target, self.fraction, size, density).outline_in(
            size,
            density,
            layout_direction,
        )
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
        let fraction = state.fraction();
        morphed(&state.start.borrow(), &state.target.borrow(), fraction, size, density).outline_in(
            size,
            density,
            layout_direction,
        )
    }

    fn same(&self, other: &dyn CustomShape) -> bool {
        other.downcast_ref::<AnimatedShape>().is_some_and(|other| Rc::ptr_eq(&self.0, &other.0))
    }
}

impl AnimatedShapeState {
    /// How far along it is, read where the outline is made.
    fn fraction(&self) -> f32 {
        if self.started.get() { self.progress.get().map_or(1.0, Animatable::value) } else { 1.0 }
    }

    /// As `animateToShape`.
    async fn animate_to(&self, new_target: Shape) {
        if *self.target.borrow() == new_target {
            return;
        }
        let progress = self.progress.get_or_init(|| Animatable::new(1.0));
        self.started.set(true);
        if *self.start.borrow() == new_target {
            // Reversed before the animation finished: its progress flipped, its velocity too.
            let old_target = self.target.replace(new_target);
            *self.start.borrow_mut() = old_target;
            let (at, velocity) = (progress.value(), progress.velocity());
            progress.snap_to(1.0 - at);
            progress.animate_to_with_velocity(1.0, self.spec.clone(), -velocity).await;
        } else {
            // A new target: from what shows now, frozen, and only then if it's between two.
            let at = progress.value();
            let frozen = if at == 1.0 {
                self.target.borrow().clone()
            } else if at == 0.0 {
                self.start.borrow().clone()
            } else {
                Shape::custom(FrozenShape {
                    start: self.start.borrow().clone(),
                    target: self.target.borrow().clone(),
                    fraction: at,
                })
            };
            *self.start.borrow_mut() = frozen;
            *self.target.borrow_mut() = new_target;
            progress.snap_to(0.0);
            progress.animate_to(1.0, self.spec.clone()).await;
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
                progress: OnceCell::new(),
                started: mutable_state_of(false),
                spec,
                moved: Cell::new(false),
            })
        }
    });
    // No effect until a new target comes: each item of a lazy list composes this.
    let moved = state.moved.get() || *state.target.borrow() != current;
    state.moved.set(moved);
    compose_runtime::key(moved, || {
        if moved {
            LaunchedEffect((current.clone(), state.clone()), {
                let state = state.clone();
                async move { state.animate_to(current).await }
            });
        }
    });
    Shape::custom(AnimatedShape(state))
}
