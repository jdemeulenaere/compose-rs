//! Live `LayoutCoordinates` (docs/parity/design/placement-and-lookahead.md §4, step PL-2): what
//! they answer, where they answer, and the placement blocks that read them.

use crate::support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::*;
use support::*;

type Told = Rc<Cell<Option<LayoutCoordinates>>>;

fn told() -> Told {
    Rc::default()
}

/// `told` keeps the coordinates of what follows it.
fn keep(told: &Told) -> Modifier {
    let told = told.clone();
    Modifier.on_globally_positioned(move |coordinates: LayoutCoordinates| {
        told.set(Some(coordinates));
    })
}

fn of(told: &Told) -> LayoutCoordinates {
    told.get().expect("positioned")
}

fn sized(size: i32) -> Modifier {
    Modifier.size((size as f32).dp())
}

/// Moves what follows by `x`, `y`, as `Modifier.offset`, not reading anything itself.
fn moved(x: i32, y: i32) -> Modifier {
    Modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        scope.place(&placeable, x, y);
        scope.layout(placeable.width, placeable.height)
    })
}

/// Moves what follows by what `offset` says, reading it while it measures.
fn moved_by(offset: MutableState<(i32, i32)>) -> Modifier {
    Modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        let (x, y) = offset.get();
        scope.place(&placeable, x, y);
        scope.layout(placeable.width, placeable.height)
    })
}

/// Pads what follows by `by` on each side.
fn padded(by: i32) -> Modifier {
    Modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints.offset(-2 * by, -2 * by));
        scope.place(&placeable, by, by);
        scope.layout(placeable.width + 2 * by, placeable.height + 2 * by)
    })
}

/// A leaf whose modifier's placement block does `read` with its coordinates, counted by `runs`.
fn reading(runs: &Rc<Cell<u32>>, read: impl Fn(LayoutCoordinates) + 'static) -> Modifier {
    let runs = runs.clone();
    let read = Rc::new(read);
    Modifier
        .layout(move |scope, measurable, constraints| {
            let placeable = scope.measure(measurable, constraints);
            let (runs, read) = (runs.clone(), read.clone());
            scope.layout_with(placeable.width, placeable.height, move |placement| {
                runs.set(runs.get() + 1);
                read(placement.coordinates().expect("no alignment lines"));
                placement.place(&placeable, 0, 0);
            })
        })
        .then(sized(10))
}

/// A leaf whose policy's placement block does `read` with its coordinates.
#[composable]
fn ReadingLeaf(runs: Rc<Cell<u32>>, read: Rc<dyn Fn(LayoutCoordinates)>) {
    Layout(
        Modifier,
        measure_policy(move |scope, _, _| {
            let (runs, read) = (runs.clone(), read.clone());
            scope.layout_with(10, 10, move |placement| {
                runs.set(runs.get() + 1);
                read(placement.coordinates().expect("no alignment lines"));
            })
        }),
        callback(|| {}),
    );
}

fn close(a: Offset, b: Offset) -> bool {
    (a.x - b.x).abs() < 0.01 && (a.y - b.y).abs() < 0.01
}

// ---- What they answer ----------------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/OnGloballyPositionedTest.kt, testPositionInParent
#[test]
fn position_in_parent_is_from_the_parent_s_content_and_the_root_s_is_zero() {
    let coordinates = told();
    let c = coordinates.clone();
    let _host = host(move || {
        Stack!(modifier = padded(5).then(keep(&c)).then(sized(10)), {});
    });
    let coordinates = of(&coordinates);
    assert_eq!(coordinates.position_in_parent(), Offset::new(5.0, 5.0));
    let mut root = coordinates;
    while let Some(parent) = root.parent_layout_coordinates() {
        root = parent;
    }
    assert_eq!(root.position_in_parent(), Offset::ZERO);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/OnGloballyPositionedTest.kt, testBoundsInParent
#[test]
fn bounds_in_parent_are_from_the_parent_s_content_and_the_root_s_are_its_size() {
    let coordinates = told();
    let c = coordinates.clone();
    let _host = host(move || {
        Stack!(modifier = padded(5).then(keep(&c)).then(sized(10)), {});
    });
    let coordinates = of(&coordinates);
    assert_eq!(coordinates.bounds_in_parent(), Rect::new(5.0, 5.0, 15.0, 15.0));
    assert_eq!(coordinates.size(), IntSize::new(10, 10));
    let root = coordinates.find_root_coordinates();
    assert_eq!(root.parent_layout_coordinates(), None);
    assert_eq!(root.bounds_in_parent().left, 0.0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/OnGloballyPositionedTest.kt, testBoundsInWindow
#[test]
fn bounds_in_window_are_clipped_when_asked() {
    let coordinates = told();
    let c = coordinates.clone();
    let _host = host(move || {
        let c = c.clone();
        Stack!(modifier = Modifier.clip_to_bounds(), {
            Leaf(moved(-5, -5).then(keep(&c)).then(sized(10)));
        });
    });
    let coordinates = of(&coordinates);
    assert_eq!(coordinates.bounds_in_window(), Rect::new(0.0, 0.0, 5.0, 5.0));
    assert_eq!(coordinates.bounds_in_window_with(false), Rect::new(-5.0, -5.0, 5.0, 5.0));
}

/// Clipped by the root's bounds too, in the window: not in the root.
// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/layout/LayoutCoordinates.kt, boundsInWindow
#[test]
fn bounds_in_window_are_clipped_by_the_window() {
    let coordinates = told();
    let c = coordinates.clone();
    let _host = host(move || Leaf(moved(-5, -5).then(keep(&c)).then(sized(10))));
    let coordinates = of(&coordinates);
    assert_eq!(coordinates.bounds_in_window(), Rect::new(0.0, 0.0, 5.0, 5.0));
    assert_eq!(coordinates.bounds_in_root(), Rect::new(-5.0, -5.0, 5.0, 5.0));
}

/// A clip outside what asks isn't one on the way; one inside it is.
// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/NodeCoordinator.kt, localBoundingBoxOf
#[test]
fn a_bounding_box_is_clipped_by_the_clips_on_the_way_alone() {
    let (before, after, child) = (told(), told(), told());
    let (b, a, c) = (before.clone(), after.clone(), child.clone());
    let _host = host(move || {
        let c = c.clone();
        Stack!(modifier = sized(10).then(keep(&b)).clip_to_bounds().then(keep(&a)), {
            Leaf(moved(20, 0).then(keep(&c)).then(sized(5)));
        });
    });
    let (before, after, child) = (of(&before), of(&after), of(&child));
    assert_eq!(after.local_bounding_box_of(&child, true), Rect::new(20.0, 0.0, 25.0, 5.0));
    assert_eq!(before.local_bounding_box_of(&child, true), Rect::new(0.0, 0.0, 0.0, 0.0));
    assert_eq!(before.local_bounding_box_of(&child, false), Rect::new(20.0, 0.0, 25.0, 5.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/OnGloballyPositionedTest.kt, nestedLayoutCoordinates
#[test]
fn nested_coordinates_answer_from_any_of_them() {
    let (grandparent, child) = (told(), told());
    let (g, c) = (grandparent.clone(), child.clone());
    let _host = host(move || {
        let c = c.clone();
        Stack!(modifier = moved(10, 0).then(keep(&g)), {
            let c = c.clone();
            Stack!(modifier = moved(20, 0), {
                Leaf(moved(30, 0).then(keep(&c)).then(sized(5)));
            });
        });
    });
    let (grandparent, child) = (of(&grandparent), of(&child));
    assert_eq!(child.local_to_window(Offset::ZERO).x, 60.0);
    assert_eq!(grandparent.local_position_of(&child, Offset::ZERO).x, 50.0);
    assert_eq!(child.position_in_parent().x, 30.0);
    // And down.
    assert_eq!(child.local_position_of(&grandparent, Offset::ZERO).x, -50.0);
    assert_eq!(child.window_to_local(Offset::new(61.0, 2.0)), Offset::new(1.0, 2.0));
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/LayoutNodeTest.kt, testLocalPositionOfWithSiblings
#[test]
fn siblings_answer_through_their_parent() {
    let (first, second) = (told(), told());
    let (f, s) = (first.clone(), second.clone());
    let _host = host(move || {
        let (f, s) = (f.clone(), s.clone());
        Stack!(modifier = moved(100, 100), {
            Leaf(moved(5, 7).then(keep(&f)).then(sized(10)));
            Leaf(moved(11, 13).then(keep(&s)).then(sized(10)));
        });
    });
    let (first, second) = (of(&first), of(&second));
    assert_eq!(first.local_position_of(&second, Offset::new(1.0, 1.0)), Offset::new(7.0, 7.0));
    assert_eq!(second.local_position_of(&first, Offset::ZERO), Offset::new(-6.0, -6.0));
    assert_eq!(first.local_position_of(&first, Offset::new(3.0, 3.0)), Offset::new(3.0, 3.0));
}

/// Two coordinators of one node: through what's between them.
#[test]
fn coordinators_of_one_node_answer_through_what_s_between_them() {
    let (outer, inner) = (told(), told());
    let (o, i) = (outer.clone(), inner.clone());
    let _host = host(move || {
        Leaf(keep(&o).then(moved(3, 4)).then(padded(2)).then(keep(&i)).then(sized(10)));
    });
    let (outer, inner) = (of(&outer), of(&inner));
    assert_eq!(outer.local_position_of(&inner, Offset::ZERO), Offset::new(5.0, 6.0));
    assert_eq!(inner.local_position_of(&outer, Offset::ZERO), Offset::new(-5.0, -6.0));
    assert_eq!(inner.parent_coordinates().map(|parent| parent.size()), Some(IntSize::new(14, 14)));
    // `parent_layout_coordinates` isn't its own node's.
    assert_eq!(inner.parent_layout_coordinates(), outer.parent_layout_coordinates());
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/LayoutNodeTest.kt, nodeCoordinator_transformFrom_translation
#[test]
fn a_translation_answers_both_ways_and_in_a_matrix() {
    let (parent, child) = (told(), told());
    let (p, c) = (parent.clone(), child.clone());
    let _host = host(move || {
        let c = c.clone();
        Stack!(modifier = keep(&p), {
            Leaf(
                Modifier
                    .graphics_layer(|layer| {
                        layer.translation_x = 5.0;
                        layer.translation_y = 2.0;
                    })
                    .then(keep(&c))
                    .then(sized(10)),
            );
        });
    });
    let (parent, child) = (of(&parent), of(&child));
    assert_eq!(parent.local_position_of(&child, Offset::ZERO), Offset::new(5.0, 2.0));
    assert_eq!(child.local_position_of(&parent, Offset::ZERO), Offset::new(-5.0, -2.0));
    let mut matrix = Matrix::new();
    parent.transform_from(&child, &mut matrix);
    assert_eq!(matrix.map(Offset::ZERO), Offset::new(5.0, 2.0));
    child.transform_from(&parent, &mut matrix);
    assert_eq!(matrix.map(Offset::ZERO), Offset::new(-5.0, -2.0));
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/LayoutNodeTest.kt, nodeCoordinator_transformFrom_rotation
#[test]
fn a_rotation_answers_both_ways_and_in_a_matrix() {
    let (parent, child) = (told(), told());
    let (p, c) = (parent.clone(), child.clone());
    let _host = host(move || {
        let c = c.clone();
        Stack!(modifier = keep(&p), {
            Leaf(sized(10).graphics_layer(|layer| layer.rotation_z = 90.0).then(keep(&c)));
        });
    });
    let (parent, child) = (of(&parent), of(&child));
    // About the center (5, 5): its top left goes to its top right.
    assert!(close(parent.local_position_of(&child, Offset::ZERO), Offset::new(10.0, 0.0)));
    assert!(close(child.local_position_of(&parent, Offset::new(10.0, 0.0)), Offset::ZERO));
    let mut matrix = Matrix::new();
    parent.transform_from(&child, &mut matrix);
    assert!(close(matrix.map(Offset::ZERO), Offset::new(10.0, 0.0)));
    child.transform_from(&parent, &mut matrix);
    assert!(close(matrix.map(Offset::new(10.0, 0.0)), Offset::ZERO));
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/LayoutNodeTest.kt, nodeCoordinator_transformFrom_scale
#[test]
fn a_scale_answers_both_ways_and_in_a_matrix() {
    let (parent, child) = (told(), told());
    let (p, c) = (parent.clone(), child.clone());
    let _host = host(move || {
        let c = c.clone();
        Stack!(modifier = keep(&p), {
            Leaf(
                sized(10)
                    .graphics_layer(|layer| {
                        layer.scale_x = 2.0;
                        layer.scale_y = 3.0;
                    })
                    .then(keep(&c)),
            );
        });
    });
    let (parent, child) = (of(&parent), of(&child));
    // About the center: (0, 0) is 5 from it each way.
    assert!(close(parent.local_position_of(&child, Offset::ZERO), Offset::new(-5.0, -10.0)));
    assert!(close(child.local_position_of(&parent, Offset::new(-5.0, -10.0)), Offset::ZERO));
    let mut matrix = Matrix::new();
    parent.transform_from(&child, &mut matrix);
    assert!(close(matrix.map(Offset::new(5.0, 5.0)), Offset::new(5.0, 5.0)));
    assert!(close(matrix.map(Offset::ZERO), Offset::new(-5.0, -10.0)));
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/LayoutNodeTest.kt, nodeCoordinator_transformFrom_cousins
#[test]
fn cousins_answer_through_their_common_ancestor_alone() {
    let (first, second) = (told(), told());
    let (f, s) = (first.clone(), second.clone());
    let _host = host(move || {
        let (f, s) = (f.clone(), s.clone());
        // A scale of nothing above the common ancestor, which an answer through the root can't undo.
        Stack!(modifier = sized(100).graphics_layer(|layer| layer.scale_x = 0.0), {
            let (f, s) = (f.clone(), s.clone());
            Stack!(modifier = moved(10, 0), {
                Leaf(keep(&f).then(sized(5)));
            });
            Stack!(modifier = moved(30, 0), {
                Leaf(keep(&s).then(sized(5)));
            });
        });
    });
    let (first, second) = (of(&first), of(&second));
    assert_eq!(first.local_position_of(&second, Offset::ZERO), Offset::new(20.0, 0.0));
    let mut matrix = Matrix::new();
    first.transform_from(&second, &mut matrix);
    assert_eq!(matrix.map(Offset::ZERO), Offset::new(20.0, 0.0));
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/LayoutNodeTest.kt, testPxLocalToGlobalUsesOwnerPosition
#[test]
fn the_screen_is_the_window_s_origin_away() {
    let coordinates = told();
    let c = coordinates.clone();
    let mut host = host(move || Leaf(moved(5, 5).then(keep(&c)).then(sized(10))));
    host.set_window_origin(Offset::new(100.0, 200.0));
    host.frame();
    let coordinates = of(&coordinates);
    assert_eq!(coordinates.position_in_root(), Offset::new(5.0, 5.0));
    assert_eq!(coordinates.position_in_window(), Offset::new(5.0, 5.0));
    assert_eq!(coordinates.position_on_screen(), Offset::new(105.0, 205.0));
    assert_eq!(coordinates.screen_to_local(Offset::new(105.0, 205.0)), Offset::ZERO);
    let mut matrix = Matrix::new();
    coordinates.transform_to_screen(&mut matrix);
    assert_eq!(matrix.map(Offset::ZERO), Offset::new(105.0, 205.0));
}

/// A live handle: what's kept answers what the tree says now, not what it said then.
#[test]
fn kept_coordinates_answer_what_is_there_now() {
    let offset = mutable_state_of((1, 2));
    let coordinates = told();
    let (c, o) = (coordinates.clone(), offset.handle());
    let mut host = host(move || Leaf(moved_by(o).then(keep(&c)).then(sized(10))));
    let kept = of(&coordinates);
    assert_eq!(kept.position_in_root(), Offset::new(1.0, 2.0));
    offset.set((30, 40));
    host.frame();
    assert_eq!(kept.position_in_root(), Offset::new(30.0, 40.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/OnGloballyPositionedTest.kt, coordinatesAreNotAttachedWhenNodeIsRemovedFromHierarchy
#[test]
fn coordinates_of_a_node_removed_are_not_attached() {
    let needed = mutable_state_of(true);
    let coordinates = told();
    let (c, n) = (coordinates.clone(), needed.handle());
    let mut host = host(move || {
        if n.get() {
            Leaf(keep(&c));
        }
    });
    let kept = of(&coordinates);
    assert!(kept.is_attached());
    needed.set(false);
    host.frame();
    assert!(!kept.is_attached());
    assert_eq!(kept.size(), IntSize::ZERO);
    assert_eq!(kept.position_in_root(), Offset::ZERO);
}

/// Deactivated for reuse, its modifiers are detached, as a coordinator's `isAttached`.
#[test]
fn coordinates_of_a_node_deactivated_are_not_attached() {
    let active = mutable_state_of(true);
    let coordinates = told();
    let (c, a) = (coordinates.clone(), active.handle());
    let mut host = host(move || {
        let c = c.clone();
        ReusableContentHost(a.get(), move || {
            let c = c.clone();
            ReusableContent(0, move || Leaf(keep(&c).then(sized(1))));
        });
    });
    let kept = of(&coordinates);
    assert!(kept.is_attached());
    active.set(false);
    host.frame();
    assert!(!kept.is_attached());
    active.set(true);
    host.frame();
    assert!(of(&coordinates).is_attached());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/OnGloballyPositionedTest.kt, coordinatesAreNotAttachedWhenModifierIsNotUsedAnymore
#[test]
fn coordinates_of_a_layout_modifier_gone_are_not_attached() {
    let used = mutable_state_of(true);
    let coordinates = told();
    let (c, u) = (coordinates.clone(), used.handle());
    let mut host = host(move || {
        Leaf(if u.get() { keep(&c).then(moved(0, 0)) } else { Modifier });
    });
    let kept = of(&coordinates);
    assert!(kept.is_attached());
    used.set(false);
    host.frame();
    assert!(!kept.is_attached());
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/LayoutNodeTest.kt, nodeCoordinatorSameWithReplacementModifier
#[test]
fn coordinates_of_one_coordinator_are_equal() {
    let (first, second, third) = (told(), told(), told());
    let (a, b, c) = (first.clone(), second.clone(), third.clone());
    let _host = host(move || {
        Leaf(keep(&a).then(keep(&b)).then(padded(1)).then(keep(&c)).then(sized(1)));
    });
    assert_eq!(of(&first), of(&second), "the same layout modifier's");
    assert_ne!(of(&first), of(&third));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/OnGloballyPositionedTest.kt, onPositionedIsCalledWhenComposeContainerPositionChanged
#[test]
fn the_window_s_origin_moving_tells_position_aware_nodes() {
    let calls = Rc::new(Cell::new(0));
    let c = calls.clone();
    let mut host = host(move || {
        let c = c.clone();
        Leaf(
            Modifier
                .on_globally_positioned(move |_: LayoutCoordinates| c.set(c.get() + 1))
                .then(sized(1)),
        );
    });
    assert_eq!(calls.get(), 1);
    host.set_window_origin(Offset::new(1.0, 0.0));
    host.frame();
    assert_eq!(calls.get(), 2);
    host.set_window_origin(Offset::new(1.0, 0.0));
    host.frame();
    assert_eq!(calls.get(), 2);
}

/// Asked in a composable: nothing borrows the tree while it composes.
#[test]
fn coordinates_answer_in_a_composition() {
    let coordinates = told();
    let again = mutable_state_of(0);
    let seen = Rc::new(Cell::new(Offset::ZERO));
    let (c, s, a) = (coordinates.clone(), seen.clone(), again.handle());
    let mut host = host(move || {
        a.get();
        if let Some(coordinates) = c.get() {
            s.set(coordinates.position_in_root());
        }
        Leaf(moved(3, 4).then(keep(&c)).then(sized(1)));
    });
    again.set(1);
    host.frame();
    assert_eq!(seen.get(), Offset::new(3.0, 4.0));
}

/// Asked in each of a pass's places: all of them read the tree in the middle of that pass.
#[test]
fn coordinates_answer_in_measuring_drawing_and_a_layer_s_block() {
    let kept = told();
    let seen: Rc<RefCell<Vec<(&'static str, Offset)>>> = Rc::default();
    let (k, s) = (kept.clone(), seen.clone());
    let again = mutable_state_of(0);
    let a = again.handle();
    let mut host = host(move || {
        let (k, s) = (k.clone(), s.clone());
        let at =
            move |what: &'static str, seen: &Rc<RefCell<Vec<(&'static str, Offset)>>>, k: &Told| {
                if let Some(coordinates) = k.get() {
                    seen.borrow_mut().push((what, coordinates.position_in_root()));
                }
            };
        let (s1, s2, s3, k1, k2, k3) =
            (s.clone(), s.clone(), s.clone(), k.clone(), k.clone(), k.clone());
        a.get();
        Leaf(
            moved(3, 4)
                .then(keep(&k))
                .then(Modifier.layout(move |scope, measurable, constraints| {
                    at("measure", &s1, &k1);
                    let placeable = scope.measure(measurable, constraints);
                    scope.place(&placeable, 0, 0);
                    scope.layout(placeable.width, placeable.height)
                }))
                .then(sized(10))
                .draw_behind(move |_| at("draw", &s2, &k2))
                .graphics_layer(move |_| at("layer", &s3, &k3)),
        );
    });
    again.set(1);
    host.frame();
    let seen = seen.borrow();
    for what in ["measure", "draw", "layer"] {
        assert!(
            seen.iter().any(|(name, at)| *name == what && *at == Offset::new(3.0, 4.0)),
            "{what}: {seen:?}"
        );
    }
}

/// A node's callback asks in the pass that tells it.
#[test]
fn a_node_told_it_was_placed_or_positioned_asks_there() {
    #[derive(Default)]
    struct Seen {
        placed: Cell<Offset>,
        positioned: Cell<Offset>,
    }
    struct Asking(Rc<Seen>);
    impl ModifierNode for Asking {
        fn as_layout_aware(&mut self) -> Option<&mut dyn LayoutAwareModifierNode> {
            Some(self)
        }
        fn as_global_position_aware(&mut self) -> Option<&mut dyn GlobalPositionAwareModifierNode> {
            Some(self)
        }
    }
    impl LayoutAwareModifierNode for Asking {
        fn on_placed(&mut self, coordinates: LayoutCoordinates, _: &mut NodeContext<'_>) {
            self.0.placed.set(coordinates.position_in_root());
        }
    }
    impl GlobalPositionAwareModifierNode for Asking {
        fn on_globally_positioned(
            &mut self,
            coordinates: LayoutCoordinates,
            _: &mut NodeContext<'_>,
        ) {
            self.0.positioned.set(coordinates.position_in_root());
        }
    }
    struct Element(Rc<Seen>);
    impl ModifierNodeElement for Element {
        type Node = Asking;
        fn create(&self) -> Asking {
            Asking(self.0.clone())
        }
        fn update(&self, _: &mut Asking) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    let seen: Rc<Seen> = Rc::default();
    let s = seen.clone();
    let _host = host(move || Leaf(moved(3, 4).then_custom(Element(s.clone())).then(sized(1))));
    assert_eq!(seen.placed.get(), Offset::new(3.0, 4.0));
    assert_eq!(seen.positioned.get(), Offset::new(3.0, 4.0));
}

// ---- A node's own -------------------------------------------------------------------------------

#[derive(Default)]
struct Coordinates {
    handle: Cell<Option<NodeHandle>>,
    in_attach: Cell<Option<LayoutCoordinates>>,
    in_attach_position: Cell<Offset>,
}

struct AskingNode(Rc<Coordinates>);

impl ModifierNode for AskingNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.0.handle.set(Some(context.handle()));
        let coordinates = context.require_layout_coordinates();
        self.0.in_attach_position.set(coordinates.position_in_root());
        self.0.in_attach.set(Some(coordinates));
    }
}

struct AskingElement(Rc<Coordinates>);

impl ModifierNodeElement for AskingElement {
    type Node = AskingNode;
    fn create(&self) -> AskingNode {
        AskingNode(self.0.clone())
    }
    fn update(&self, _: &mut AskingNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/RequireLayoutCoordinatesTest.kt, requireLayoutCoordinates_returnsCoordinates_whenAttached
#[test]
fn a_node_s_coordinates_are_of_where_it_is() {
    let asked: Rc<Coordinates> = Rc::default();
    let a = asked.clone();
    let _host = host(move || {
        Leaf(moved(10, 20).then(sized(30)).then_custom(AskingElement(a.clone())));
    });
    let coordinates = asked.handle.get().expect("attached").require_layout_coordinates();
    assert!(coordinates.is_attached());
    assert_eq!(coordinates.position_in_root(), Offset::new(10.0, 20.0));
    assert_eq!(coordinates.size(), IntSize::new(30, 30));
    assert_eq!(asked.in_attach.get(), Some(coordinates), "what its attach was given");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/RequireLayoutCoordinatesTest.kt, requireLayoutCoordinates_returnsCoordinatesFromNearestLayoutModifier
#[test]
fn a_node_s_coordinates_are_of_the_nearest_layout_modifier_after_it() {
    let asked: Rc<Coordinates> = Rc::default();
    let a = asked.clone();
    let _host = host(move || {
        Leaf(sized(10).then_custom(AskingElement(a.clone())).required_size(5.dp()));
    });
    let coordinates = asked.handle.get().expect("attached").require_layout_coordinates();
    assert_eq!(coordinates.size(), IntSize::new(5, 5));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/RequireLayoutCoordinatesTest.kt, requireLayoutCoordinates_throws_afterDetached
#[test]
#[should_panic(expected = "isn't attached")]
fn a_node_detached_has_no_coordinates() {
    let attached = mutable_state_of(true);
    let asked: Rc<Coordinates> = Rc::default();
    let (a, on) = (asked.clone(), attached.handle());
    let mut host = host(move || {
        Leaf(if on.get() { Modifier.then_custom(AskingElement(a.clone())) } else { Modifier });
    });
    let handle = asked.handle.get().expect("attached");
    attached.set(false);
    host.frame();
    handle.require_layout_coordinates();
}

// ---- Placement blocks that ask ------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, coordinatesWhilePlacing
#[test]
fn a_block_knows_where_it_is_placed() {
    let offset = mutable_state_of((40, 30));
    let (runs, at) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(Offset::ZERO)));
    let (r, a, o) = (runs.clone(), at.clone(), offset.handle());
    let mut host = host(move || {
        let a = a.clone();
        Leaf(
            moved_by(o).then(reading(&r, move |coordinates| a.set(coordinates.position_in_root()))),
        );
    });
    assert_eq!(at.get(), Offset::new(40.0, 30.0));
    assert_eq!(runs.get(), 1);
    offset.set((0, 0));
    host.frame();
    assert_eq!(at.get(), Offset::ZERO);
    assert_eq!(runs.get(), 2);
    offset.set((7, 9));
    host.frame();
    assert_eq!(at.get(), Offset::new(7.0, 9.0));
    assert_eq!(runs.get(), 3);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, parentCoordinateChangeCausesRelayout
#[test]
fn a_parent_moving_runs_a_block_that_asked() {
    let offset = mutable_state_of((0, 0));
    let runs = Rc::new(Cell::new(0));
    let (r, o) = (runs.clone(), offset.handle());
    let mut host = host(move || {
        let r = r.clone();
        Stack!(modifier = moved_by(o), {
            Leaf(reading(&r, |coordinates| {
                coordinates.position_in_root();
            }));
        });
    });
    assert_eq!(runs.get(), 1);
    offset.set((1, 2));
    host.frame();
    assert_eq!(runs.get(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, grandParentCoordateChangeCausesRelayout
#[test]
fn a_grandparent_moving_runs_a_block_that_asked() {
    let offset = mutable_state_of((0, 0));
    let runs = Rc::new(Cell::new(0));
    let (r, o) = (runs.clone(), offset.handle());
    let mut host = host(move || {
        let r = r.clone();
        Stack!(modifier = moved_by(o), {
            let r = r.clone();
            Stack!({
                Leaf(reading(&r, |coordinates| {
                    coordinates.position_in_root();
                }));
            });
        });
    });
    offset.set((1, 2));
    host.frame();
    assert_eq!(runs.get(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, newlyAddedStillUpdated
#[test]
fn a_block_added_later_still_runs_when_its_parent_moves() {
    let (offset, show) = (mutable_state_of((0, 0)), mutable_state_of(false));
    let runs = Rc::new(Cell::new(0));
    let (r, o, s) = (runs.clone(), offset.handle(), show.handle());
    let mut host = host(move || {
        let r = r.clone();
        Stack!(modifier = moved_by(o), {
            Leaf(sized(5));
            if s.get() {
                Leaf(reading(&r, |coordinates| {
                    coordinates.position_in_root();
                }));
            }
        });
    });
    show.set(true);
    host.frame();
    assert_eq!(runs.get(), 1);
    offset.set((1, 2));
    host.frame();
    assert_eq!(runs.get(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, removedStopsUpdating
#[test]
fn a_block_no_longer_asking_stops_running() {
    let (offset, ask) = (mutable_state_of((0, 0)), mutable_state_of(true));
    let runs = Rc::new(Cell::new(0));
    let (r, o, a) = (runs.clone(), offset.handle(), ask.handle());
    let mut host = host(move || {
        let r = r.clone();
        Stack!(modifier = moved_by(o), {
            Leaf(reading(&r, move |coordinates| {
                if a.get() {
                    coordinates.position_in_root();
                }
            }));
        });
    });
    assert_eq!(runs.get(), 1);
    offset.set((10, 5));
    host.frame();
    assert_eq!(runs.get(), 2);
    ask.set(false);
    host.frame();
    assert_eq!(runs.get(), 3, "what it read changed");
    offset.set((0, 0));
    host.frame();
    assert_eq!(runs.get(), 3);
}

/// A reader removed is forgotten: its parent moving later asks nothing of it.
#[test]
fn a_block_removed_is_forgotten() {
    let (offset, show) = (mutable_state_of((0, 0)), mutable_state_of(true));
    let runs = Rc::new(Cell::new(0));
    let (r, o, s) = (runs.clone(), offset.handle(), show.handle());
    let mut host = host(move || {
        let r = r.clone();
        Stack!(modifier = moved_by(o), {
            Leaf(sized(5));
            if s.get() {
                Leaf(reading(&r, |coordinates| {
                    coordinates.position_in_root();
                }));
            }
        });
    });
    show.set(false);
    host.frame();
    offset.set((1, 1));
    host.frame();
    assert_eq!(runs.get(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, ancestorLayerChangesCausesPlacement
#[test]
fn a_layer_of_an_ancestor_changing_runs_a_block_that_asked() {
    let offset = mutable_state_of(0.0f32);
    let (runs, at) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(Offset::ZERO)));
    let (r, a, o) = (runs.clone(), at.clone(), offset.handle());
    let mut host = host(move || {
        let (r, a) = (r.clone(), a.clone());
        Stack!(modifier = Modifier.graphics_layer(move |layer| layer.translation_x = o.get()), {
            let a = a.clone();
            Leaf(reading(&r, move |coordinates| a.set(coordinates.position_in_root())));
        });
    });
    assert_eq!(runs.get(), 1);
    offset.set(1.0);
    host.frame();
    assert_eq!(runs.get(), 2);
    assert_eq!(at.get(), Offset::new(1.0, 0.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, layerChangesCausesPlacement
#[test]
fn its_own_layer_changing_runs_a_block_that_asked() {
    let offset = mutable_state_of(0.0f32);
    let runs = Rc::new(Cell::new(0));
    let (r, o) = (runs.clone(), offset.handle());
    let mut host = host(move || {
        Leaf(Modifier.graphics_layer(move |layer| layer.translation_x = o.get()).then(reading(
            &r,
            |coordinates| {
                coordinates.position_in_root();
            },
        )));
    });
    assert_eq!(runs.get(), 1);
    offset.set(1.0);
    host.frame();
    assert_eq!(runs.get(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, viewPositionChangeCausesPlacement
#[test]
fn the_window_s_origin_moving_runs_a_block_that_asked() {
    let runs = Rc::new(Cell::new(0));
    let r = runs.clone();
    let mut host = host(move || {
        Leaf(reading(&r, |coordinates| {
            coordinates.position_on_screen();
        }));
    });
    assert_eq!(runs.get(), 1);
    host.set_window_origin(Offset::new(1.0, 1.0));
    host.frame();
    assert_eq!(runs.get(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, readingFromMainLayoutPolicyAfterMultipleMoves
#[test]
fn a_policy_s_block_runs_for_each_move() {
    let offset = mutable_state_of(0);
    let runs = Rc::new(Cell::new(0));
    let (r, o) = (runs.clone(), offset.handle());
    let parent = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            placement.place(&placeable, o.get(), 0);
        })
    });
    let mut host = host(move || {
        let r = r.clone();
        Layout(
            Modifier,
            parent.clone(),
            callback(move || {
                ReadingLeaf(
                    r.clone(),
                    Rc::new(|coordinates: LayoutCoordinates| {
                        coordinates.position_in_parent();
                    }),
                );
            }),
        );
    });
    runs.set(0);
    offset.set(1);
    host.frame();
    assert_eq!(runs.get(), 1);
    runs.set(0);
    offset.set(2);
    host.frame();
    assert_eq!(runs.get(), 1);
}

fn only_real_position_reads_run_a_block(in_modifier: bool) {
    type Read = fn(LayoutCoordinates);
    let reads: [(bool, &str, Read); 9] = [
        (true, "position_in_parent", |it| {
            let _ = it.position_in_parent();
        }),
        (true, "position_in_root", |it| {
            let _ = it.position_in_root();
        }),
        (true, "position_in_window", |it| {
            let _ = it.position_in_window();
        }),
        (true, "bounds_in_parent", |it| {
            let _ = it.bounds_in_parent();
        }),
        (true, "bounds_in_root", |it| {
            let _ = it.bounds_in_root();
        }),
        (true, "bounds_in_window", |it| {
            let _ = it.bounds_in_window();
        }),
        (false, "empty", |_| {}),
        (false, "size", |it| {
            let _ = it.size();
        }),
        (false, "is_attached", |it| {
            let _ = it.is_attached();
        }),
    ];
    let offset = mutable_state_of(0);
    let action = mutable_state_of(0usize);
    let runs = Rc::new(Cell::new(0));
    let (r, o, a) = (runs.clone(), offset.handle(), action.handle());
    let parent = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            placement.place(&placeable, o.get(), 0);
        })
    });
    let mut host = host(move || {
        let r = r.clone();
        Layout(
            Modifier,
            parent.clone(),
            callback(move || {
                let read =
                    Rc::new(move |coordinates: LayoutCoordinates| reads[a.get()].2(coordinates));
                if in_modifier {
                    Leaf(reading(&r, move |coordinates| read(coordinates)));
                } else {
                    ReadingLeaf(r.clone(), read);
                }
            }),
        );
    });
    for (index, (expected, what, _)) in reads.iter().enumerate() {
        action.set(index);
        host.frame();
        runs.set(0);
        offset.set(if offset.get() == 0 { 10 } else { 0 });
        host.frame();
        assert_eq!(runs.get(), u32::from(*expected), "{what}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, onlyRealPositionReadsTriggerRelayout
#[test]
fn only_a_policy_s_block_reading_a_position_runs_again() {
    only_real_position_reads_run_a_block(false);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, onlyRealPositionReadsTriggerRelayout_inModifier
#[test]
fn only_a_modifier_s_block_reading_a_position_runs_again() {
    only_real_position_reads_run_a_block(true);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, removingLayoutModifierShouldInvalidateCoordinatesOnGrandChild
#[test]
fn a_layout_modifier_going_runs_a_block_below_that_asked() {
    let on = mutable_state_of(true);
    let at = Rc::new(Cell::new(Offset::ZERO));
    let runs = Rc::new(Cell::new(0));
    let (a, r, o) = (at.clone(), runs.clone(), on.handle());
    let mut host = host(move || {
        let (a, r) = (a.clone(), r.clone());
        let layout = if o.get() { moved(10, 0) } else { Modifier };
        Stack!(modifier = layout.clone(), {
            let (a, r) = (a.clone(), r.clone());
            Stack!({
                let a = a.clone();
                Leaf(reading(&r, move |coordinates| a.set(coordinates.position_in_root())));
            });
        });
    });
    assert_eq!(at.get(), Offset::new(10.0, 0.0));
    on.set(false);
    host.frame();
    assert_eq!(at.get(), Offset::ZERO);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacementLayoutCoordinatesTest.kt, removingLayerModifierShouldInvalidateCoordinatesOnGrandChild
#[test]
fn a_layer_going_runs_a_block_below_that_asked() {
    let on = mutable_state_of(true);
    let at = Rc::new(Cell::new(Offset::ZERO));
    let runs = Rc::new(Cell::new(0));
    let (a, r, o) = (at.clone(), runs.clone(), on.handle());
    let mut host = host(move || {
        let (a, r) = (a.clone(), r.clone());
        let layer = if o.get() {
            Modifier.graphics_layer(|layer| layer.translation_x = 10.0)
        } else {
            Modifier
        };
        Stack!(modifier = layer.clone(), {
            let (a, r) = (a.clone(), r.clone());
            Stack!({
                let a = a.clone();
                Leaf(reading(&r, move |coordinates| a.set(coordinates.position_in_root())));
            });
        });
    });
    assert_eq!(at.get(), Offset::new(10.0, 0.0));
    on.set(false);
    host.frame();
    assert_eq!(at.get(), Offset::ZERO);
}

/// A block that moves what's below it, which asks: each runs once, the parent first.
#[test]
fn a_reader_placed_by_a_reader_runs_once() {
    let offset = mutable_state_of(0);
    let (parent_runs, child_runs) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let seen = Rc::new(Cell::new(Offset::ZERO));
    let (p, c, s, o) = (parent_runs.clone(), child_runs.clone(), seen.clone(), offset.handle());
    let parent = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        let p = p.clone();
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            p.set(p.get() + 1);
            let at = placement.coordinates().expect("placing").position_in_root();
            placement.place(&placeable, at.x as i32 * 2, 0);
        })
    });
    let mut host = host(move || {
        let (c, s, parent) = (c.clone(), s.clone(), parent.clone());
        Stack!(modifier = moved_by_x(o), {
            let (c, s, parent) = (c.clone(), s.clone(), parent.clone());
            Layout(
                Modifier,
                parent,
                callback(move || {
                    let s = s.clone();
                    Leaf(reading(&c, move |coordinates| s.set(coordinates.position_in_root())));
                }),
            );
        });
    });
    assert_eq!((parent_runs.get(), child_runs.get()), (1, 1));
    offset.set(5);
    host.frame();
    assert_eq!((parent_runs.get(), child_runs.get()), (2, 2));
    assert_eq!(seen.get(), Offset::new(15.0, 0.0));
    host.frame();
    assert_eq!((parent_runs.get(), child_runs.get()), (2, 2), "at rest");
}

fn moved_by_x(offset: MutableState<i32>) -> Modifier {
    Modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        scope.place(&placeable, offset.get(), 0);
        scope.layout(placeable.width, placeable.height)
    })
}

// ---- `on_placed` asking ---------------------------------------------------------------------------

fn told_at(at: &Rc<Cell<Offset>>) -> Modifier {
    let at = at.clone();
    Modifier.on_placed(move |coordinates: LayoutCoordinates| at.set(coordinates.position_in_root()))
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacedChildTest.kt, removingLayoutModifierShouldTriggerOnPlacedOnGrandChild
#[test]
fn a_layout_modifier_going_tells_on_placed_on_a_grandchild_that_asked() {
    let on = mutable_state_of(true);
    let at = Rc::new(Cell::new(Offset::ZERO));
    let (a, o) = (at.clone(), on.handle());
    let mut host = host(move || {
        let a = a.clone();
        let modifier = if o.get() { moved(10, 0) } else { Modifier };
        Stack!(modifier = modifier.clone().then(moved(10, 0)), {
            let a = a.clone();
            Stack!(modifier = sized(10), {
                Leaf(told_at(&a));
            });
        });
    });
    assert_eq!(at.get(), Offset::new(20.0, 0.0));
    on.set(false);
    host.frame();
    assert_eq!(at.get(), Offset::new(10.0, 0.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacedChildTest.kt, removingLayerModifierShouldTriggerOnPlacedOnGrandChild
#[test]
fn a_layer_going_tells_on_placed_on_a_grandchild_that_asked() {
    let on = mutable_state_of(true);
    let at = Rc::new(Cell::new(Offset::ZERO));
    let (a, o) = (at.clone(), on.handle());
    let mut host = host(move || {
        let a = a.clone();
        let layer = if o.get() {
            Modifier.graphics_layer(|layer| layer.translation_x = 10.0)
        } else {
            Modifier
        };
        Stack!(modifier = layer.clone(), {
            let a = a.clone();
            Stack!(modifier = sized(10), {
                Leaf(told_at(&a));
            });
        });
    });
    assert_eq!(at.get(), Offset::new(10.0, 0.0));
    on.set(false);
    host.frame();
    assert_eq!(at.get(), Offset::ZERO);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacedChildTest.kt, updatingLayerPropertyShouldCallCallbackOnTheSameNode
#[test]
fn a_layer_s_property_tells_on_placed_on_the_same_node() {
    let offset = mutable_state_of(0.0f32);
    let at = Rc::new(Cell::new(Offset::ZERO));
    let (a, o) = (at.clone(), offset.handle());
    let mut host = host(move || {
        Leaf(
            Modifier
                .graphics_layer(move |layer| layer.translation_x = o.get())
                .then(told_at(&a))
                .then(sized(10)),
        );
    });
    assert_eq!(at.get(), Offset::ZERO);
    offset.set(5.0);
    host.frame();
    assert_eq!(at.get(), Offset::new(5.0, 0.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacedChildTest.kt, notPlacedChildIsNotCallingPlacingBlockOnItsModifier
#[test]
fn a_child_not_placed_doesn_t_run_its_modifier_s_block() {
    let (show, measures, blocks) =
        (mutable_state_of(false), Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (s, m, b) = (show.handle(), measures.clone(), blocks.clone());
    let parent = measure_policy(|scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        scope.layout_with(placeable.width, placeable.height, |_| {})
    });
    let mut host = host(move || {
        let (m, b) = (m.clone(), b.clone());
        Layout(
            Modifier,
            parent.clone(),
            callback(move || {
                let (m, b) = (m.clone(), b.clone());
                let modifier = if s.get() {
                    Modifier.layout(move |scope, measurable, constraints| {
                        let placeable = scope.measure(measurable, constraints);
                        m.set(m.get() + 1);
                        let b = b.clone();
                        scope.layout_with(placeable.width, placeable.height, move |placement| {
                            b.set(b.get() + 1);
                            placement.place(&placeable, 0, 0);
                        })
                    })
                } else {
                    Modifier
                };
                Leaf(modifier.then(sized(10)));
            }),
        );
    });
    show.set(true);
    host.frame();
    assert_eq!((measures.get(), blocks.get()), (1, 0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacedChildTest.kt, placementIsNotCalledOnChildOfNotPlacedParent
#[test]
fn a_child_of_a_parent_no_longer_placed_doesn_t_run_its_block() {
    let (counter, place) = (mutable_state_of(0), mutable_state_of(true));
    let blocks = Rc::new(Cell::new(0));
    let (c, p, b) = (counter.handle(), place.handle(), blocks.clone());
    let conditional = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        let place = p.get();
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            if place {
                placement.place(&placeable, 0, 0);
            }
        })
    });
    let always = measure_policy(|scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            placement.place(&placeable, 0, 0);
        })
    });
    let leaf = measure_policy(move |scope, _, _| {
        c.get();
        let b = b.clone();
        scope.layout_with(50, 50, move |_| b.set(b.get() + 1))
    });
    let mut host = host(move || {
        let (always, leaf) = (always.clone(), leaf.clone());
        Layout(
            Modifier,
            conditional.clone(),
            callback(move || {
                let leaf = leaf.clone();
                Layout(
                    Modifier,
                    always.clone(),
                    callback(move || Layout(Modifier, leaf.clone(), callback(|| {}))),
                );
            }),
        );
    });
    blocks.set(0);
    counter.set(1);
    place.set(false);
    host.frame();
    assert_eq!(blocks.get(), 0);
}

// ---- Motion frames of reference ------------------------------------------------------------------

/// Left out only when asked, and only what was placed so.
// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/layout/LayoutCoordinates.kt, localPositionOf with includeMotionFrameOfReference
#[test]
fn what_s_placed_under_a_motion_frame_is_left_out_when_asked() {
    let (outer, inner) = (told(), told());
    let (o, i) = (outer.clone(), inner.clone());
    let scrolled = measure_policy(|scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        scope.with_motion_frame_of_reference_placement(|scope| scope.place(&placeable, 0, -50));
        scope.layout(placeable.width, placeable.height)
    });
    let _host = host(move || {
        let i = i.clone();
        Layout(
            keep(&o),
            scrolled.clone(),
            callback(move || {
                Leaf(moved(0, 7).then(keep(&i)).then(sized(5)));
            }),
        );
    });
    let (outer, inner) = (of(&outer), of(&inner));
    assert_eq!(outer.local_position_of(&inner, Offset::ZERO), Offset::new(0.0, -43.0));
    assert_eq!(outer.local_position_of_with(&inner, Offset::ZERO, true), Offset::new(0.0, -43.0));
    assert_eq!(outer.local_position_of_with(&inner, Offset::ZERO, false), Offset::new(0.0, 7.0));
    assert_eq!(inner.local_position_of_with(&outer, Offset::ZERO, false), Offset::new(0.0, -7.0));
    assert!(inner.parent_layout_coordinates().is_some());
    assert!(!inner.introduces_motion_frame_of_reference());
}

/// A node placed under one tells, and stops telling once it's placed otherwise.
#[test]
fn a_motion_frame_is_of_the_last_placing() {
    let motion = mutable_state_of(true);
    let kept = told();
    let (k, m) = (kept.clone(), motion.handle());
    let policy = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        if m.get() {
            scope.with_motion_frame_of_reference_placement(|scope| scope.place(&placeable, 0, 3));
        } else {
            scope.place(&placeable, 0, 3);
        }
        scope.layout(placeable.width, placeable.height)
    });
    let mut host = host(move || {
        let k = k.clone();
        Layout(Modifier, policy.clone(), callback(move || Leaf(keep(&k).then(sized(5)))));
    });
    let kept = of(&kept);
    assert!(kept.introduces_motion_frame_of_reference());
    motion.set(false);
    host.frame();
    assert!(!kept.introduces_motion_frame_of_reference());
}
