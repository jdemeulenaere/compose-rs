//! Phase D review: a measure policy panicking once corrupts the tree for good.
//!
//! `measure_layer` takes a node's children out of it while its policy runs (`mem::take`), and
//! puts them back after. A panic anywhere below unwinds through every ancestor's
//! `measure_layer`, dropping each one's children list: afterwards the root has no children,
//! though the nodes are alive in the arena and in the composition, and the composition isn't
//! poisoned. Later frames lay out and draw nothing, and dropping the host panics in
//! `UiTree::remove` (it drains a range past the empty list); at thread exit or while
//! unwinding, that aborts the process (this binary has one test for that reason).

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

mod review_d_support;
mod support;

use std::rc::Rc;

use compose_runtime::*;
use compose_ui::*;
use review_d_support::*;

thread_local! {
    static POLICY: Rc<dyn MeasurePolicy> = Rc::new(PanickyPolicy);
}

fn content() {
    Layout(
        Modifier,
        POLICY.with(Rc::clone),
        callback(|| {
            support::Leaf(Modifier.size(10.dp()));
            support::Leaf(Modifier.size(10.dp()));
        }),
    );
}

#[test]
fn a_panicking_measure_policy_leaves_the_tree_intact_and_the_host_droppable() {
    let mut host = support::host(content);
    set_fail(true);
    host.resize(IntSize::new(300, 300));
    assert!(quiet(|| host.frame()).is_err());
    set_fail(false);
    host.resize(IntSize::new(200, 100));
    host.frame();
    let after = host.dump_tree();
    let fresh = support::host(content).dump_tree();
    // Dropping the host panics: catch that, so the assertions below report.
    let dropped = quiet(move || drop(host)).is_ok();
    // Fresh: the root, the column and its two leaves. After the panic: the root alone.
    assert_eq!(after, fresh);
    assert!(dropped, "dropping the host panicked");
}
