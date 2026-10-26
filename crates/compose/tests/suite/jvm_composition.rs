//! Compose's `JvmCompositionTests`, adapted for the ui types: a composable given an unchanged
//! unspecified `Dp` skips (the float ones are in compose-runtime's suite; `DpSize` isn't a
//! `ComposeArg` and there's no `DpOffset` yet: the ui area's).

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose::runtime::platform::NodeTree;
use compose::runtime::*;
use compose::ui::Dp;

#[composable]
fn WithDpParameter(value: Dp, count: Rc<Cell<u32>>) {
    count.set(count.get() + 1);
    let _ = value;
}

fn unspecified_argument_skips(child: fn(Rc<Cell<u32>>)) {
    let composition = Composition::new(NodeTree::<()>::new());
    let trigger = mutable_state_of(false);
    let (parent, count) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (t, p, c) = (trigger.handle(), parent.clone(), count.clone());
    composition.set_content(move || {
        t.get();
        p.set(p.get() + 1);
        child(c.clone());
    });
    assert_eq!((parent.get(), count.get()), (1, 1));
    trigger.set(true);
    composition.recompose();
    assert_eq!((parent.get(), count.get()), (2, 1), "the child skipped");
}

// Compose: compose/runtime/runtime/src/nonEmulatorJvmTest/kotlin/androidx/compose/runtime/JvmCompositionTests.kt, dpUnspecifiedArgumentSkips
#[test]
fn an_unspecified_dp_argument_skips() {
    unspecified_argument_skips(|count| {
        WithDpParameter(std::hint::black_box(Dp::UNSPECIFIED), count)
    });
}
