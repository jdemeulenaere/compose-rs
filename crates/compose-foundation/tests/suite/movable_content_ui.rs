//! Movable content on a real tree (D1305): its nodes move with their layout, input and effects, and
//! what's read under a `BoxWithConstraints` follows the locals where it's moved to.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::UnitExt;

const TAG: &str = "moved";

/// Repeated moves between two places: bounds follow, clicks land, what it remembered stays and its
/// effect isn't run again.
#[test]
fn content_moves_with_its_bounds_clicks_and_effects() {
    let (clicks, effects, disposals, made) = (
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(0)),
    );
    let in_second = mutable_state_of(false);
    let (c, e, d, m, second) =
        (clicks.clone(), effects.clone(), disposals.clone(), made.clone(), in_second.handle());
    let content = movable_content_of(move || {
        let c = c.clone();
        let (e, d, m) = (e.clone(), d.clone(), m.clone());
        remember(move || m.set(m.get() + 1));
        DisposableEffect((), move |scope| {
            e.set(e.get() + 1);
            scope.on_dispose(move || d.set(d.get() + 1))
        });
        Box!(
            modifier = Modifier
                .size((20.dp(), 20.dp()))
                .clickable(move || c.set(c.get() + 1))
                .test_tag(TAG)
        );
    });
    let mut ui = compose_test(move || {
        let content = content.clone();
        let moved_there = second.get();
        Box!({
            let first = content.clone();
            Box!(modifier = Modifier.offset(0.dp(), 0.dp()), {
                if !moved_there {
                    first();
                }
            });
            let then = content.clone();
            Box!(modifier = Modifier.offset(50.dp(), 30.dp()), {
                if moved_there {
                    then();
                }
            });
        });
    });
    for round in 0..4 {
        let there = round % 2 == 1;
        let (left, top) = if there { (50.dp(), 30.dp()) } else { (0.dp(), 0.dp()) };
        in_second.set(there);
        ui.wait_for_idle();
        ui.on_node_with_tag(TAG).assert_position_in_root_is_equal_to(left, top);
        ui.on_node_with_tag(TAG).perform_click();
        ui.wait_for_idle();
        assert_eq!(clicks.get(), round + 1, "the click lands where it is, round {round}");
    }
    assert_eq!(made.get(), 1, "what it remembered moved with it");
    assert_eq!((effects.get(), disposals.get()), (1, 0), "its effect stays");
}

static MOVED_LOCAL: CompositionLocal<i32> = CompositionLocal::new_static("moved.local", || 0);

/// Under a `BoxWithConstraints` in moved content: what its content reads follows where it is, also
/// with the value changing in the frame of the move.
#[test]
fn box_with_constraints_reads_the_locals_where_its_moved() {
    let seen = Rc::new(Cell::new(0));
    #[composable]
    fn Reads(seen: Rc<Cell<i32>>) {
        seen.set(MOVED_LOCAL.current());
    }
    let (in_second, first_value) = (mutable_state_of(false), mutable_state_of(1));
    let s = seen.clone();
    let content = movable_content_of(move || {
        let s = s.clone();
        BoxWithConstraints!(move |_| Reads!(s.clone()));
    });
    let (second, value) = (in_second.handle(), first_value.handle());
    let mut ui = compose_test(move || {
        let moved_there = second.get();
        let (first, then) = (content.clone(), content.clone());
        CompositionLocalProvider(MOVED_LOCAL.provides(value.get()), move || {
            let first = first.clone();
            Box!({
                if !moved_there {
                    first();
                }
            });
        });
        CompositionLocalProvider(MOVED_LOCAL.provides(100), move || {
            let then = then.clone();
            Box!({
                if moved_there {
                    then();
                }
            });
        });
    });
    ui.wait_for_idle();
    assert_eq!(seen.get(), 1);
    in_second.set(true);
    ui.wait_for_idle();
    assert_eq!(seen.get(), 100);
    first_value.set(2);
    in_second.set(false);
    ui.wait_for_idle();
    assert_eq!(seen.get(), 2, "the value and the move, in one frame");
}
