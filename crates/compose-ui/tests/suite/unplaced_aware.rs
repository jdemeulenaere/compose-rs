//! `UnplacedAwareModifierNode`, as Compose's `UnplacedAwareModifierNodeTest`.

use crate::support;

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::*;
use support::*;

struct Unplaced(Rc<Cell<u32>>);

impl ModifierNode for Unplaced {
    fn as_unplaced_aware(&mut self) -> Option<&mut dyn UnplacedAwareModifierNode> {
        Some(self)
    }
}

impl UnplacedAwareModifierNode for Unplaced {
    fn on_unplaced(&mut self, context: &mut NodeContext<'_>) {
        assert!(context.is_attached());
        self.0.set(self.0.get() + 1);
    }
}

struct UnplacedElement(Rc<Cell<u32>>);

impl ModifierNodeElement for UnplacedElement {
    type Node = Unplaced;
    fn create(&self) -> Unplaced {
        Unplaced(self.0.clone())
    }
    fn update(&self, node: &mut Unplaced) {
        node.0 = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

fn unplaced(count: &Rc<Cell<u32>>) -> Modifier {
    Modifier.size(10.dp()).then_custom(UnplacedElement(count.clone()))
}

/// A parent placing its child while `place` says.
fn placing(place: MutableState<bool>) -> Rc<dyn MeasurePolicy> {
    measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            if place.get() {
                placement.place(&placeable, 0, 0);
            }
        })
    })
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/UnplacedAwareModifierNodeTest.kt, calledWhenNotPlacedByParentAnymore
#[test]
fn a_node_no_longer_placed_is_told() {
    let place = mutable_state_of(true);
    let count: Rc<Cell<u32>> = Rc::default();
    let (p, c) = (place.handle(), count.clone());
    let policy = placing(p);
    let mut host = host(move || {
        let c = c.clone();
        Layout(Modifier, policy.clone(), callback(move || Leaf(unplaced(&c))));
    });
    assert_eq!(count.get(), 0);
    place.set(false);
    host.frame();
    assert_eq!(count.get(), 1);
    host.frame();
    assert_eq!(count.get(), 1, "once");
    // Placed again, and not again: told again.
    place.set(true);
    host.frame();
    assert_eq!(count.get(), 1);
    place.set(false);
    host.frame();
    assert_eq!(count.get(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/UnplacedAwareModifierNodeTest.kt, calledWhenNotComposedAnymore
#[test]
fn a_node_removed_is_told() {
    let shown = mutable_state_of(true);
    let count: Rc<Cell<u32>> = Rc::default();
    let (s, c) = (shown.handle(), count.clone());
    let mut host = host(move || {
        if s.get() {
            Leaf(unplaced(&c));
        }
    });
    assert_eq!(count.get(), 0);
    shown.set(false);
    host.frame();
    assert_eq!(count.get(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/UnplacedAwareModifierNodeTest.kt, calledWhenDeactivated
#[test]
fn a_node_deactivated_is_told() {
    let active = mutable_state_of(true);
    let count: Rc<Cell<u32>> = Rc::default();
    let (a, c) = (active.handle(), count.clone());
    let mut host = host(move || {
        let c = c.clone();
        ReusableContentHost(a.get(), move || {
            let c = c.clone();
            ReusableContent(0, move || Leaf(unplaced(&c)));
        });
    });
    assert_eq!(count.get(), 0);
    active.set(false);
    host.frame();
    assert_eq!(count.get(), 1);
}

/// What's under a parent no longer placed isn't placed either, as `markSubtreeAsNotPlaced`.
#[test]
fn a_node_under_one_no_longer_placed_is_told() {
    let place = mutable_state_of(true);
    let count: Rc<Cell<u32>> = Rc::default();
    let (p, c) = (place.handle(), count.clone());
    let policy = placing(p);
    let mut host = host(move || {
        let c = c.clone();
        Layout(
            Modifier,
            policy.clone(),
            callback(move || {
                let c = c.clone();
                Stack!({
                    Leaf(unplaced(&c));
                });
            }),
        );
    });
    place.set(false);
    host.frame();
    assert_eq!(count.get(), 1);
}

/// One that never was placed has nothing to hear, removed or not.
#[test]
fn a_node_never_placed_is_told_nothing() {
    let shown = mutable_state_of(true);
    let count: Rc<Cell<u32>> = Rc::default();
    let (s, c) = (shown.handle(), count.clone());
    let never = measure_policy(|scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        scope.layout(placeable.width, placeable.height)
    });
    let mut host = host(move || {
        if s.get() {
            let c = c.clone();
            Layout(Modifier, never.clone(), callback(move || Leaf(unplaced(&c))));
        }
    });
    shown.set(false);
    host.frame();
    assert_eq!(count.get(), 0);
}

/// A delegate hears too.
#[test]
fn a_delegate_is_told() {
    struct Owner(Delegates, Rc<Cell<u32>>);
    impl ModifierNode for Owner {
        fn delegates(&mut self) -> Option<&mut Delegates> {
            Some(&mut self.0)
        }
        fn on_attach(&mut self, context: &mut NodeContext<'_>) {
            self.0.delegate_attached(Unplaced(self.1.clone()), context);
        }
    }
    struct OwnerElement(Rc<Cell<u32>>);
    impl ModifierNodeElement for OwnerElement {
        type Node = Owner;
        fn create(&self) -> Owner {
            Owner(Delegates::new(), self.0.clone())
        }
        fn update(&self, _: &mut Owner) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    let place = mutable_state_of(true);
    let count: Rc<Cell<u32>> = Rc::default();
    let (p, c) = (place.handle(), count.clone());
    let policy = placing(p);
    let mut host = host(move || {
        let c = c.clone();
        Layout(
            Modifier,
            policy.clone(),
            callback(move || Leaf(Modifier.size(1.dp()).then_custom(OwnerElement(c.clone())))),
        );
    });
    place.set(false);
    host.frame();
    assert_eq!(count.get(), 1);
}
