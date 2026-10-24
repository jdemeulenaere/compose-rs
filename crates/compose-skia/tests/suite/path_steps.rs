//! After a review (2387674): `Path::op`'s steps (no `PathOperator` on the test's thread: Android's
//! and headless's way) run as Replayer.kt runs them, against Compose on Android, which computes
//! each `op` eagerly with Skia's path ops. Regions are compared with Skia's XOR.

use compose::ui::{Offset, Path, PathCommand, PathData, PathFillType, PathOperation, Rect};
use compose_skia::skia_safe::{self, PathBuilder};

fn sk_fill(fill: PathFillType) -> skia_safe::PathFillType {
    match fill {
        PathFillType::NonZero => skia_safe::PathFillType::Winding,
        PathFillType::EvenOdd => skia_safe::PathFillType::EvenOdd,
    }
}

fn sk_op(op: PathOperation) -> skia_safe::PathOp {
    match op {
        PathOperation::Difference => skia_safe::PathOp::Difference,
        PathOperation::Intersect => skia_safe::PathOp::Intersect,
        PathOperation::Union => skia_safe::PathOp::Union,
        PathOperation::Xor => skia_safe::PathOp::XOR,
        PathOperation::ReverseDifference => skia_safe::PathOp::ReverseDifference,
    }
}

fn append(builder: &mut PathBuilder, commands: &[PathCommand]) {
    for command in commands {
        match *command {
            PathCommand::MoveTo(p) => {
                builder.move_to((p.x, p.y));
            }
            PathCommand::LineTo(p) => {
                builder.line_to((p.x, p.y));
            }
            PathCommand::QuadTo(c, p) => {
                builder.quad_to((c.x, c.y), (p.x, p.y));
            }
            PathCommand::CubicTo(a, b, p) => {
                builder.cubic_to((a.x, a.y), (b.x, b.y), (p.x, p.y));
            }
            PathCommand::Close => {
                builder.close();
            }
        }
    }
}

fn with_fill(path: &skia_safe::Path, fill: skia_safe::PathFillType) -> skia_safe::Path {
    let mut builder = PathBuilder::new_path(path);
    builder.set_fill_type(fill);
    builder.detach()
}

/// As Replayer.kt's `readPath`.
fn replay(data: &PathData) -> skia_safe::Path {
    let base = data.evaluate_steps(
        |operand| {
            let mut builder = PathBuilder::new();
            append(&mut builder, operand.commands());
            builder.set_fill_type(sk_fill(operand.fill_type()));
            builder.detach()
        },
        |first, second, op| skia_safe::op(&first, &second, sk_op(op)).unwrap_or(first),
        |before, first, second, op| skia_safe::op(&first, &second, sk_op(op)).unwrap_or(before),
        |first, second| {
            let mut builder = PathBuilder::new_path(&first);
            builder.add_path(&second, None);
            builder.set_fill_type(first.fill_type());
            builder.detach()
        },
        |top, fill| *top = with_fill(top, sk_fill(fill)),
    );
    let mut builder = match &base {
        Some(base) => PathBuilder::new_path(base),
        None => PathBuilder::new(),
    };
    append(&mut builder, data.commands());
    builder.set_fill_type(sk_fill(data.fill_type()));
    builder.detach()
}

/// Compose on Android: a Skia path changed eagerly.
#[derive(Clone)]
struct Eager(skia_safe::Path);

impl Eager {
    fn rect(rect: Rect) -> Eager {
        let path = Path::new();
        path.add_rect(rect);
        Eager(replay(&path.data()))
    }
    fn op(&mut self, first: &skia_safe::Path, second: &skia_safe::Path, op: PathOperation) {
        if let Some(result) = skia_safe::op(first, second, sk_op(op)) {
            self.0 = result;
        }
    }
    fn fill(&mut self, fill: PathFillType) {
        self.0 = with_fill(&self.0, sk_fill(fill));
    }
    fn add(&mut self, other: &skia_safe::Path) {
        let mut builder = PathBuilder::new_path(&self.0);
        builder.add_path(other, None);
        builder.set_fill_type(self.0.fill_type());
        self.0 = builder.detach();
    }
}

fn same_region(ours: &Path, theirs: &Eager, what: &str) {
    let ours = replay(&ours.data());
    let xor = skia_safe::op(&ours, &theirs.0, skia_safe::PathOp::XOR).expect("xor");
    let bounds = xor.compute_tight_bounds();
    let area = bounds.width() * bounds.height();
    assert!(
        xor.is_empty() || area < 1e-3,
        "{what}: regions differ by {bounds:?}\nours {:?}\ntheirs {:?}",
        ours.bounds(),
        theirs.0.bounds()
    );
}

fn rect(l: f32, t: f32, r: f32, b: f32) -> Path {
    let path = Path::new();
    path.add_rect(Rect::new(l, t, r, b));
    path
}

fn sk(path: &Path) -> skia_safe::Path {
    replay(&path.data())
}

#[test]
fn chains_and_swapped_operations_match_eager_skia() {
    use PathOperation::*;
    let p = rect(0.0, 0.0, 10.0, 10.0);
    let mut e = Eager::rect(Rect::new(0.0, 0.0, 10.0, 10.0));
    let operands = [
        (rect(5.0, 5.0, 15.0, 15.0), Union, false),
        (rect(-5.0, -5.0, 30.0, 30.0), Difference, true), // p = C - p
        (rect(2.0, 2.0, 4.0, 40.0), Xor, false),
        (rect(0.0, 0.0, 3.0, 3.0), ReverseDifference, true), // p = p - E
        (rect(1.0, 1.0, 25.0, 25.0), Intersect, true),
        (rect(0.0, 0.0, 6.0, 6.0), Difference, false),
        (rect(-10.0, -10.0, 50.0, 50.0), ReverseDifference, false),
    ];
    for (index, (other, op, swapped)) in operands.iter().enumerate() {
        let before = e.0.clone();
        if *swapped {
            p.op(other, &p, *op);
            e.op(&sk(other), &before, *op);
        } else {
            p.op(&p, other, *op);
            e.op(&before, &sk(other), *op);
        }
        same_region(&p, &e, &format!("step {index} {op:?} swapped {swapped}"));
    }
    assert_eq!(p.data().steps().len(), 1 + 2 * operands.len(), "steps, not computed");
}

/// Fill types between operations: a path whose contours overlap after an `add_path`, its fill
/// type set, then combined again.
#[test]
fn fill_types_between_operations_match_eager_skia() {
    for fill in [PathFillType::NonZero, PathFillType::EvenOdd] {
        let p = rect(0.0, 0.0, 10.0, 10.0);
        let mut e = Eager::rect(Rect::new(0.0, 0.0, 10.0, 10.0));
        let x = rect(20.0, 0.0, 30.0, 10.0);
        p.op(&p, &x, PathOperation::Union);
        let before = e.0.clone();
        e.op(&before, &sk(&x), PathOperation::Union);
        // An overlapping square, same winding: a hole with even-odd.
        let q = rect(2.0, 2.0, 8.0, 8.0);
        p.add_path(&q, Offset::ZERO);
        e.add(&sk(&q));
        p.set_fill_type(fill);
        e.fill(fill);
        same_region(&p, &e, &format!("after add_path, {fill:?}"));
        let y = rect(25.0, 5.0, 40.0, 20.0);
        p.op(&p, &y, PathOperation::Union);
        let before = e.0.clone();
        e.op(&before, &sk(&y), PathOperation::Union);
        same_region(&p, &e, &format!("then union, {fill:?}"));
        let z = rect(-5.0, 3.0, 50.0, 4.0);
        p.op(&z, &p, PathOperation::Difference);
        let before = e.0.clone();
        e.op(&sk(&z), &before, PathOperation::Difference);
        same_region(&p, &e, &format!("then swapped difference, {fill:?}"));
        // Commands after the operation, then another operation (steps and commands).
        p.move_to(60.0, 0.0);
        p.line_to(70.0, 0.0);
        p.line_to(70.0, 10.0);
        p.close();
        let mut tri = PathBuilder::new_path(&e.0);
        tri.move_to((60.0, 0.0)).line_to((70.0, 0.0)).line_to((70.0, 10.0)).close();
        tri.set_fill_type(e.0.fill_type());
        e.0 = tri.detach();
        same_region(&p, &e, &format!("then a contour, {fill:?}"));
        let w = rect(0.0, 0.0, 65.0, 100.0);
        p.op(&p, &w, PathOperation::Intersect);
        let before = e.0.clone();
        e.op(&before, &sk(&w), PathOperation::Intersect);
        same_region(&p, &e, &format!("then intersect, {fill:?}"));
        // Moved, then combined with itself.
        p.translate(Offset::new(3.0, 4.0));
        let moved = with_fill(&e.0.make_offset((3.0, 4.0)), e.0.fill_type());
        e.0 = moved;
        same_region(&p, &e, &format!("then translate, {fill:?}"));
        p.add_path(&p.clone(), Offset::new(1.0, 0.0));
        let shifted = e.0.make_offset((1.0, 0.0));
        e.add(&shifted);
        same_region(&p, &e, &format!("then add itself, {fill:?}"));
    }
}

/// An operand with even-odd fill (a hole) combined into a path: its fill type travels.
#[test]
fn operand_fill_types_match_eager_skia() {
    let holed = rect(0.0, 0.0, 10.0, 10.0);
    holed.add_rect(Rect::new(3.0, 3.0, 7.0, 7.0));
    holed.set_fill_type(PathFillType::EvenOdd);
    let p = rect(5.0, -5.0, 20.0, 20.0);
    let mut e = Eager(sk(&p));
    p.op(&holed, &p, PathOperation::Union);
    let before = e.0.clone();
    e.op(&sk(&holed), &before, PathOperation::Union);
    same_region(&p, &e, "holed operand");
    let combined = Path::combine(PathOperation::Xor, &p, &holed);
    let mut ce = Eager(sk(&holed));
    ce.op(&e.0, &sk(&holed), PathOperation::Xor);
    same_region(&combined, &ce, "combine of a step path");
}
