//! After a review (2387674): compose-rs `Path` semantics against Skia's (Android's `android.graphics.Path`
//! and Compose Desktop are both Skia). "Ours" is our commands replayed into a Skia path, as
//! Replayer.kt replays them into an `android.graphics.Path`.

use compose::ui::{Offset, Path, PathCommand, Rect};
use compose_skia::skia_safe::{self, PathBuilder, PathVerb};

fn replay(path: &Path) -> skia_safe::Path {
    let mut skia = PathBuilder::new();
    for command in path.data().commands() {
        match *command {
            PathCommand::MoveTo(p) => {
                skia.move_to((p.x, p.y));
            }
            PathCommand::LineTo(p) => {
                skia.line_to((p.x, p.y));
            }
            PathCommand::QuadTo(c, p) => {
                skia.quad_to((c.x, c.y), (p.x, p.y));
            }
            PathCommand::CubicTo(a, b, p) => {
                skia.cubic_to((a.x, a.y), (b.x, b.y), (p.x, p.y));
            }
            PathCommand::Close => {
                skia.close();
            }
        }
    }
    skia.detach()
}

fn moves(path: &skia_safe::Path) -> usize {
    path.verbs().iter().filter(|verb| matches!(verb, PathVerb::Move)).count()
}

fn sk_rect(rect: Rect) -> skia_safe::Rect {
    skia_safe::Rect::new(rect.left, rect.top, rect.right, rect.bottom)
}

/// `Path().isConvex` on Android / Skia.
#[test]
fn empty_path_convexity_matches_skia() {
    let skia = PathBuilder::new().detach();
    assert_eq!(Path::new().is_convex(), skia.is_convex(), "empty path");
}

/// `addArc(oval, 45, 360)`: Skia's arcTo of a whole turn from a non-quarter start.
#[test]
fn whole_turn_arc_from_non_quarter_start_matches_skia() {
    let oval = Rect::new(0.0, 0.0, 100.0, 100.0);
    let ours = Path::new();
    ours.add_arc(oval, 45.0, 360.0);
    let mut theirs = PathBuilder::new();
    theirs.add_arc(sk_rect(oval), 45.0, 360.0);
    let theirs = theirs.detach();
    let ours = replay(&ours);
    assert_eq!(ours.bounds(), theirs.bounds());
}

/// `arcTo(oval, 0, 360, true)`: Skia's arcTo of exactly a whole turn (Android's well-known
/// "a 360° arcTo draws nothing").
#[test]
fn whole_turn_arc_to_matches_skia() {
    let oval = Rect::new(0.0, 0.0, 100.0, 100.0);
    let ours = Path::new();
    ours.arc_to(oval, 0.0, 360.0, true);
    let mut theirs = PathBuilder::new();
    theirs.arc_to(sk_rect(oval), 0.0, 360.0, true);
    let theirs = theirs.detach();
    let ours = replay(&ours);
    assert_eq!(ours.bounds(), theirs.bounds());
}

/// `addPath` of a path whose first command is a line (no move): Skia starts the added contour at
/// the source's implicit (0, 0) moved by the offset.
#[test]
fn add_path_of_a_path_without_a_move_matches_skia() {
    let (ours, source) = (Path::new(), Path::new());
    ours.move_to(50.0, 50.0);
    ours.line_to(60.0, 60.0);
    source.line_to(10.0, 10.0);
    ours.add_path(&source, Offset::new(5.0, 5.0));

    let mut source_skia = PathBuilder::new();
    source_skia.line_to((10.0, 10.0));
    let source_skia = source_skia.detach();
    let mut theirs = PathBuilder::new();
    theirs.move_to((50.0, 50.0)).line_to((60.0, 60.0));
    theirs.add_path_with_offset(&source_skia, (5.0, 5.0), None);
    let theirs = theirs.detach();
    let ours = replay(&ours);
    assert_eq!((moves(&ours), ours.points()), (moves(&theirs), theirs.points()));
}
