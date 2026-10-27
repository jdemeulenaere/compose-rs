//! Helpers for the phase D review's panic tests: a measure policy and a layout modifier node
//! that panic on demand. Each panic test is a test binary of its own, because the corrupt tree
//! they expose panics again when the host is dropped, which aborts the process if it happens
//! while unwinding or at thread exit.

#![allow(dead_code)]

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

use compose_ui::*;

thread_local! {
    pub static FAIL: Cell<bool> = const { Cell::new(false) };
    pub static DETACHED: Cell<u32> = const { Cell::new(0) };
}

pub fn set_fail(fail: bool) {
    FAIL.with(|cell| cell.set(fail));
}

/// Runs `f`, catching its panic without printing it.
pub fn quiet<R>(f: impl FnOnce() -> R) -> std::thread::Result<R> {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = catch_unwind(AssertUnwindSafe(f));
    std::panic::set_hook(hook);
    result
}

/// A column that panics while `FAIL` is set.
pub struct PanickyPolicy;

impl MeasurePolicy for PanickyPolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        if FAIL.with(Cell::get) {
            panic!("review: measure failed");
        }
        let (mut width, mut y) = (0, 0);
        for &measurable in measurables {
            let placeable = scope.measure(measurable, constraints.loosen());
            scope.place(&placeable, 0, y);
            y += placeable.height;
            width = width.max(placeable.width);
        }
        scope.layout(width, y)
    }
}

/// A layout modifier that panics while `FAIL` is set, and counts its detaches.
pub struct FragileElement;

pub struct FragileNode;

impl ModifierNodeElement for FragileElement {
    type Node = FragileNode;
    fn create(&self) -> FragileNode {
        FragileNode
    }
    fn update(&self, _: &mut Self::Node) {}
    fn same(&self, _other: &Self) -> bool {
        true
    }
}

impl ModifierNode for FragileNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn on_release(&mut self, _: &mut ReleaseContext<'_>) {
        DETACHED.with(|detached| detached.set(detached.get() + 1));
    }
}

impl LayoutModifierNode for FragileNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        if FAIL.with(Cell::get) {
            panic!("review: layout modifier failed");
        }
        let placeable = scope.measure(content, constraints);
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }
}
