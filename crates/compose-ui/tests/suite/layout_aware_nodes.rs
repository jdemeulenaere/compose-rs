//! Nodes told their layer was measured and placed, and where they are
//! (docs/parity/design/modifier-nodes.md §3.11, step MN-11).

use crate::support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::*;
use support::*;

/// What a layout-aware node was told.
#[derive(Clone, Default)]
struct Told {
    remeasured: Rc<RefCell<Vec<IntSize>>>,
    placed: Rc<RefCell<Vec<Offset>>>,
}

impl PartialEq for Told {
    fn eq(&self, other: &Told) -> bool {
        Rc::ptr_eq(&self.placed, &other.placed)
    }
}

impl Told {
    fn remeasured(&self) -> Vec<IntSize> {
        std::mem::take(&mut *self.remeasured.borrow_mut())
    }
    fn placed(&self) -> Vec<Offset> {
        std::mem::take(&mut *self.placed.borrow_mut())
    }
    fn counts(&self) -> (usize, usize) {
        (self.remeasured().len(), self.placed().len())
    }
}

struct Aware(Told);

impl ModifierNode for Aware {
    fn as_layout_aware(&mut self) -> Option<&mut dyn LayoutAwareModifierNode> {
        Some(self)
    }
}

impl LayoutAwareModifierNode for Aware {
    fn on_remeasured(&mut self, size: IntSize, _: &mut NodeContext<'_>) {
        self.0.remeasured.borrow_mut().push(size);
    }
    fn on_placed(&mut self, coordinates: LayoutCoordinates, _: &mut NodeContext<'_>) {
        self.0.placed.borrow_mut().push(coordinates.position_in_root());
    }
}

struct AwareElement(Told);

impl ModifierNodeElement for AwareElement {
    type Node = Aware;
    fn create(&self) -> Aware {
        Aware(self.0.clone())
    }
    fn update(&self, node: &mut Aware) {
        node.0 = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0.placed, &other.0.placed)
    }
}

fn aware(told: &Told) -> Modifier {
    Modifier.then_custom(AwareElement(told.clone()))
}

/// A `DelegatingNode` over layout-aware delegates, keeping its handle to delegate more.
struct Owner(Delegates, Rc<Cell<Option<NodeHandle>>>);

impl ModifierNode for Owner {
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.0)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.1.set(Some(context.handle()));
    }
}

struct OwnerElement(Vec<Told>, Rc<Cell<Option<NodeHandle>>>);

impl ModifierNodeElement for OwnerElement {
    type Node = Owner;
    fn create(&self) -> Owner {
        let mut delegates = Delegates::new();
        for told in &self.0 {
            delegates.delegate(Aware(told.clone()));
        }
        Owner(delegates, self.1.clone())
    }
    fn update(&self, _: &mut Owner) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

fn owner(delegates: &[&Told]) -> Modifier {
    let told = delegates.iter().map(|told| (*told).clone()).collect();
    Modifier.then_custom(OwnerElement(told, Rc::default()))
}

/// A `Box`: a stack of `content`.
fn stack(modifier: Modifier, content: impl Fn() + 'static) {
    Stack(modifier, callback(content));
}

fn px(value: i32) -> Dp {
    Dp(value as f32)
}

fn size(value: i32) -> IntSize {
    IntSize::new(value, value)
}

fn sized_content(value: i32) {
    Leaf(Modifier.required_size(px(value)));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/OnSizeChangedTest.kt, addedModifierNode
#[test]
fn an_added_node_is_told_and_the_others_see_the_layout() {
    let add = mutable_state_of(false);
    let (one, two) = (Told::default(), Told::default());
    let (a, o, t) = (add.handle(), one.clone(), two.clone());
    let mut host = host(move || {
        let mut modifier = Modifier.padding(px(10)).then(aware(&o));
        if a.get() {
            modifier = modifier.then(aware(&t));
        }
        stack(modifier, || sized_content(10));
    });
    assert_eq!(one.remeasured(), [size(10)]);
    assert_eq!(one.placed().len(), 1);
    add.set(true);
    host.frame();
    assert_eq!(one.remeasured(), [size(10)]);
    assert_eq!(one.placed().len(), 1);
    assert_eq!(two.remeasured(), [size(10)]);
    assert_eq!(two.placed().len(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/OnSizeChangedTest.kt, removedModifierNode
#[test]
fn removing_a_node_tells_the_others_nothing() {
    let add = mutable_state_of(true);
    let (one, two) = (Told::default(), Told::default());
    let (a, o, t) = (add.handle(), one.clone(), two.clone());
    let mut host = host(move || {
        let mut modifier = Modifier.padding(px(10)).then(aware(&o));
        if a.get() {
            modifier = modifier.then(aware(&t));
        }
        stack(modifier, || sized_content(10));
    });
    assert_eq!(one.counts(), (1, 1));
    assert_eq!(two.remeasured(), [size(10)]);
    assert_eq!(two.placed().len(), 1);
    add.set(false);
    host.frame();
    assert_eq!(one.counts(), (0, 0));
    assert_eq!(two.counts(), (0, 0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/OnSizeChangedTest.kt, updatedModifierNode
#[test]
fn an_updated_node_lays_out_again_for_all_of_its_layout_node() {
    let told = mutable_state_of(Told::default());
    let (other, first) = (Told::default(), told.get());
    let (t, o) = (told.handle(), other.clone());
    let mut host = host(move || {
        let modifier = Modifier.padding(px(10)).then(aware(&t.get())).then(aware(&o));
        stack(modifier, || sized_content(10));
    });
    assert_eq!(first.remeasured(), [size(10)]);
    assert_eq!(first.placed().len(), 1);
    assert_eq!(other.counts(), (1, 1));
    let updated = Told::default();
    told.set(updated.clone());
    host.frame();
    assert_eq!(updated.remeasured(), [size(10)]);
    assert_eq!(updated.placed().len(), 1);
    assert_eq!(first.counts(), (0, 0));
    assert_eq!(other.counts(), (1, 1));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/OnSizeChangedTest.kt, lazilyDelegatedModifierNode
#[test]
fn a_node_delegated_to_later_is_told() {
    let (one, two) = (Told::default(), Told::default());
    let handle: Rc<Cell<Option<NodeHandle>>> = Rc::default();
    let (o, h) = (one.clone(), handle.clone());
    let mut host = host(move || {
        let modifier = Modifier
            .padding(px(10))
            .then(aware(&o))
            .then_custom(OwnerElement(Vec::new(), h.clone()));
        stack(modifier, || sized_content(10));
    });
    assert_eq!(one.remeasured(), [size(10)]);
    assert_eq!(one.placed().len(), 1);
    let t = two.clone();
    handle
        .get()
        .expect("attached")
        .with_node(move |node: &mut Owner, context| {
            node.0.delegate_attached(Aware(t), context);
        })
        .expect("the tree is free between frames");
    host.frame();
    assert_eq!(two.remeasured(), [size(10)]);
    assert_eq!(two.placed().len(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/OnSizeChangedTest.kt, delegatedSizeChanged
#[test]
fn a_delegate_is_told_each_new_size() {
    let content = mutable_state_of(10);
    let told = Told::default();
    let (c, t) = (content.handle(), told.clone());
    let mut host = host(move || {
        let c = c;
        stack(Modifier.padding(px(10)).then(owner(&[&t])), move || sized_content(c.get()));
    });
    assert_eq!(told.remeasured(), [size(10)]);
    content.set(20);
    host.frame();
    assert_eq!(told.remeasured(), [size(20)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/OnSizeChangedTest.kt, multipleDelegatedSizeChanged
#[test]
fn each_delegate_is_told_each_new_size() {
    let content = mutable_state_of(10);
    let (one, two) = (Told::default(), Told::default());
    let (c, a, b) = (content.handle(), one.clone(), two.clone());
    let mut host = host(move || {
        let c = c;
        stack(Modifier.padding(px(10)).then(owner(&[&a, &b])), move || sized_content(c.get()));
    });
    assert_eq!(one.remeasured(), [size(10)]);
    assert_eq!(two.remeasured(), [size(10)]);
    content.set(20);
    host.frame();
    assert_eq!(one.remeasured(), [size(20)]);
    assert_eq!(two.remeasured(), [size(20)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/OnSizeChangedTest.kt, multipleDelegatedOnPlaced
#[test]
fn each_delegate_is_told_it_was_placed_once() {
    let padding = mutable_state_of(10);
    let (one, two) = (Told::default(), Told::default());
    let (p, a, b) = (padding.handle(), one.clone(), two.clone());
    let mut host = host(move || {
        stack(Modifier.padding(px(p.get())).then(owner(&[&a, &b])), || sized_content(10));
    });
    assert_eq!(one.placed(), [Offset::new(10.0, 10.0)]);
    assert_eq!(two.placed(), [Offset::new(10.0, 10.0)]);
    padding.set(20);
    host.frame();
    assert_eq!(one.placed(), [Offset::new(20.0, 20.0)]);
    assert_eq!(two.placed(), [Offset::new(20.0, 20.0)]);
}

/// As `NodeCoordinator.onMeasured`'s `Snapshot.withoutReadObservation`.
#[test]
fn what_on_remeasured_reads_is_not_observed() {
    let read = mutable_state_of(0);
    let measured = Rc::new(Cell::new(0));

    struct Reads(MutableState<i32>);
    impl ModifierNode for Reads {
        fn as_measured_size_aware(&mut self) -> Option<&mut dyn MeasuredSizeAwareModifierNode> {
            Some(self)
        }
    }
    impl MeasuredSizeAwareModifierNode for Reads {
        fn on_remeasured(&mut self, _: IntSize, _: &mut NodeContext<'_>) {
            self.0.get();
        }
    }
    struct ReadsElement(MutableState<i32>);
    impl ModifierNodeElement for ReadsElement {
        type Node = Reads;
        fn create(&self) -> Reads {
            Reads(self.0)
        }
        fn update(&self, _: &mut Reads) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }

    let (r, m) = (read.handle(), measured.clone());
    let policy = measure_policy(move |scope, _, _| {
        m.set(m.get() + 1);
        scope.layout(1, 1)
    });
    let outer = measure_policy(move |scope, measurables, constraints| {
        // Measured inside this layout's own observation.
        let placeables: Vec<_> =
            measurables.iter().map(|m| scope.measure(*m, constraints)).collect();
        for placeable in &placeables {
            scope.place(placeable, 0, 0);
        }
        scope.layout(1, 1)
    });
    let mut host = host(move || {
        let policy = policy.clone();
        Layout(
            Modifier,
            outer.clone(),
            callback(move || {
                Layout(Modifier.then_custom(ReadsElement(r)), policy.clone(), callback(|| {}));
            }),
        );
    });
    assert_eq!(measured.get(), 1);
    read.set(1);
    host.frame();
    assert_eq!(measured.get(), 1, "a read in on_remeasured measures nothing");
}

#[test]
fn a_node_invalidating_in_on_remeasured_is_measured_again_in_the_frame() {
    let measured = Rc::new(Cell::new(0));

    struct Invalidates(Rc<Cell<bool>>);
    impl ModifierNode for Invalidates {
        fn as_layout_aware(&mut self) -> Option<&mut dyn LayoutAwareModifierNode> {
            Some(self)
        }
    }
    impl LayoutAwareModifierNode for Invalidates {
        fn on_remeasured(&mut self, _: IntSize, context: &mut NodeContext<'_>) {
            if !self.0.replace(true) {
                context.invalidate_measurement();
            }
        }
    }
    struct InvalidatesElement;
    impl ModifierNodeElement for InvalidatesElement {
        type Node = Invalidates;
        fn create(&self) -> Invalidates {
            Invalidates(Rc::default())
        }
        fn update(&self, _: &mut Invalidates) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }

    let m = measured.clone();
    let policy = measure_policy(move |scope, _, _| {
        m.set(m.get() + 1);
        scope.layout(1, 1)
    });
    let _host = host(move || {
        Layout(Modifier.then_custom(InvalidatesElement), policy.clone(), callback(|| {}));
    });
    assert_eq!(measured.get(), 2);
}

#[test]
fn a_node_may_delegate_in_on_remeasured() {
    let attached = Rc::new(Cell::new(0));

    struct Attaches(Rc<Cell<u32>>);
    impl ModifierNode for Attaches {
        fn on_attach(&mut self, _: &mut NodeContext<'_>) {
            self.0.set(self.0.get() + 1);
        }
    }
    struct Delegating(Rc<Cell<u32>>, Delegates);
    impl ModifierNode for Delegating {
        fn as_layout_aware(&mut self) -> Option<&mut dyn LayoutAwareModifierNode> {
            Some(self)
        }
        fn delegates(&mut self) -> Option<&mut Delegates> {
            Some(&mut self.1)
        }
    }
    impl LayoutAwareModifierNode for Delegating {
        fn on_remeasured(&mut self, _: IntSize, context: &mut NodeContext<'_>) {
            if self.1.nodes().next().is_none() {
                self.1.delegate_attached(Attaches(self.0.clone()), context);
            }
        }
    }
    struct DelegatingElement(Rc<Cell<u32>>);
    impl ModifierNodeElement for DelegatingElement {
        type Node = Delegating;
        fn create(&self) -> Delegating {
            Delegating(self.0.clone(), Delegates::new())
        }
        fn update(&self, _: &mut Delegating) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }

    let a = attached.clone();
    let _host = host(move || Leaf(Modifier.then_custom(DelegatingElement(a.clone())).size(px(1))));
    assert_eq!(attached.get(), 1, "attached in the frame");
}

// ---- onPlaced ----------------------------------------------------------------------------------

#[test]
fn modifier_on_placed_reports_where_it_is_outer_first() {
    let order = Rc::new(RefCell::new(Vec::new()));
    let (a, b) = (order.clone(), order.clone());
    let _host = host(move || {
        let (a, b) = (a.clone(), b.clone());
        stack(
            Modifier.padding(px(5)).on_placed(move |coordinates: LayoutCoordinates| {
                a.borrow_mut().push(("parent", coordinates.position_in_root()))
            }),
            move || {
                let b = b.clone();
                Leaf(Modifier.size(px(10)).on_placed(move |coordinates: LayoutCoordinates| {
                    b.borrow_mut().push(("child", coordinates.position_in_root()))
                }));
            },
        );
    });
    assert_eq!(
        *order.borrow(),
        [("parent", Offset::new(5.0, 5.0)), ("child", Offset::new(5.0, 5.0))]
    );
}

#[test]
fn a_node_before_a_layout_modifier_is_told_of_its_layer() {
    let (before, after) = (Told::default(), Told::default());
    let (b, a) = (before.clone(), after.clone());
    let _host = host(move || {
        Leaf(aware(&b).padding(px(10)).then(aware(&a)).size(px(5)));
    });
    assert_eq!(before.remeasured(), [size(25)]);
    assert_eq!(before.placed(), [Offset::ZERO]);
    assert_eq!(after.remeasured(), [size(5)]);
    assert_eq!(after.placed(), [Offset::new(10.0, 10.0)]);
}

#[test]
fn a_node_only_placed_again_tells_the_layers_inside_it() {
    let at = mutable_state_of(1);
    let (outer, inner) = (Told::default(), Told::default());
    let (a, o, i) = (at.handle(), outer.clone(), inner.clone());
    let policy = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        scope.place(&placeable, a.get(), 0);
        scope.layout(placeable.width, placeable.height)
    });
    let mut host = host(move || {
        let (o, i) = (o.clone(), i.clone());
        Layout(
            Modifier,
            policy.clone(),
            callback(move || Leaf(aware(&o).padding(px(10)).then(aware(&i)).size(px(5)))),
        );
    });
    assert_eq!(outer.placed(), [Offset::new(1.0, 0.0)]);
    assert_eq!(inner.placed(), [Offset::new(11.0, 10.0)]);
    let remeasured = (outer.remeasured().len(), inner.remeasured().len());
    at.set(2);
    host.frame();
    assert_eq!(outer.placed(), [Offset::new(2.0, 0.0)]);
    assert_eq!(inner.placed(), [Offset::new(12.0, 10.0)]);
    assert_eq!(remeasured, (1, 1));
    assert!(outer.remeasured().is_empty() && inner.remeasured().is_empty());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacedChildTest.kt, onPlacedIsNotCalledOnNotPlacedChildUsedByAlignmentLinesCalculation
#[test]
fn a_child_measured_but_not_placed_is_told_nothing() {
    let told = Told::default();
    let t = told.clone();
    let policy = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        scope.layout(placeable.width, placeable.height)
    });
    let _host = host(move || {
        let t = t.clone();
        Layout(Modifier, policy.clone(), callback(move || Leaf(aware(&t).size(px(5)))));
    });
    assert_eq!(told.remeasured(), [size(5)]);
    assert!(told.placed().is_empty());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacedChildTest.kt, onPlacedIsNotCalledOnNotPlacedChildUsedByAlignmentLinesCalculation_nested
#[test]
fn a_grandchild_of_a_child_not_placed_is_told_nothing() {
    let told = Told::default();
    let t = told.clone();
    let policy = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        scope.layout(placeable.width, placeable.height)
    });
    let _host = host(move || {
        let t = t.clone();
        Layout(
            Modifier,
            policy.clone(),
            callback(move || {
                let t = t.clone();
                stack(Modifier, move || Leaf(aware(&t).size(px(5))));
            }),
        );
    });
    assert!(told.placed().is_empty());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacedChildTest.kt, onPlacedIsNotCalledDuringAlignmentLinesCalculation
#[test]
fn a_placed_child_is_told_once_where_it_ends_up() {
    let told = Told::default();
    let t = told.clone();
    let policy = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        scope.place(&placeable, 10, 10);
        scope.layout(placeable.width, placeable.height)
    });
    let _host = host(move || {
        let t = t.clone();
        Layout(Modifier, policy.clone(), callback(move || Leaf(aware(&t).size(px(5)))));
    });
    assert_eq!(told.placed(), [Offset::new(10.0, 10.0)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/PlacedChildTest.kt, addingChildWithBaselineLater_isDisplayedDrawnAndOnPlacedIsCalled
#[test]
fn a_child_added_later_is_told_once() {
    let need = mutable_state_of(false);
    let told = Told::default();
    let (n, t) = (need.handle(), told.clone());
    let policy = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        scope.place(&placeable, 10, 10);
        scope.layout(placeable.width, placeable.height)
    });
    let mut host = host(move || {
        let t = t.clone();
        Layout(
            Modifier,
            policy.clone(),
            callback(move || {
                let t = t.clone();
                stack(Modifier, move || {
                    if n.get() {
                        Leaf(aware(&t).size(px(5)));
                    }
                });
            }),
        );
    });
    need.set(true);
    host.frame();
    assert_eq!(told.placed(), [Offset::new(10.0, 10.0)]);
}

#[test]
fn a_child_placed_at_last_is_told_then() {
    let place = mutable_state_of(false);
    let told = Told::default();
    let (p, t) = (place.handle(), told.clone());
    let policy = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        if p.get() {
            scope.place(&placeable, 3, 4);
        }
        scope.layout(placeable.width, placeable.height)
    });
    let mut host = host(move || {
        let t = t.clone();
        Layout(Modifier, policy.clone(), callback(move || Leaf(aware(&t).size(px(5)))));
    });
    assert!(told.placed().is_empty());
    place.set(true);
    host.frame();
    assert_eq!(told.placed(), [Offset::new(3.0, 4.0)]);
    assert!(told.remeasured().len() == 1, "measured once, when composed");
}

#[test]
fn a_node_told_it_was_placed_may_delegate_and_write_state() {
    let shown = mutable_state_of(0);
    let sizes = Rc::new(RefCell::new(Vec::new()));
    let attached = Rc::new(Cell::new(false));

    struct Attaches(Rc<Cell<bool>>);
    impl ModifierNode for Attaches {
        fn on_attach(&mut self, _: &mut NodeContext<'_>) {
            self.0.set(true);
        }
    }
    struct Grows {
        shown: MutableState<i32>,
        sizes: Rc<RefCell<Vec<IntSize>>>,
        attached: Rc<Cell<bool>>,
        delegates: Delegates,
    }
    impl ModifierNode for Grows {
        fn as_layout_aware(&mut self) -> Option<&mut dyn LayoutAwareModifierNode> {
            Some(self)
        }
        fn delegates(&mut self) -> Option<&mut Delegates> {
            Some(&mut self.delegates)
        }
    }
    impl LayoutAwareModifierNode for Grows {
        fn on_placed(&mut self, coordinates: LayoutCoordinates, context: &mut NodeContext<'_>) {
            self.sizes.borrow_mut().push(coordinates.size());
            if self.delegates.nodes().next().is_none() {
                self.delegates.delegate_attached(Attaches(self.attached.clone()), context);
                self.shown.set(1);
            }
        }
    }
    struct GrowsElement(MutableState<i32>, Rc<RefCell<Vec<IntSize>>>, Rc<Cell<bool>>);
    impl ModifierNodeElement for GrowsElement {
        type Node = Grows;
        fn create(&self) -> Grows {
            Grows {
                shown: self.0,
                sizes: self.1.clone(),
                attached: self.2.clone(),
                delegates: Delegates::new(),
            }
        }
        fn update(&self, _: &mut Grows) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }

    let (h, s, a) = (shown.handle(), sizes.clone(), attached.clone());
    let _host = host(move || {
        let size = px(if h.get() == 0 { 5 } else { 7 });
        Leaf(Modifier.then_custom(GrowsElement(h, s.clone(), a.clone())).size(size));
    });
    assert!(attached.get());
    assert_eq!(*sizes.borrow(), [size(5), size(7)], "composed and laid out in the same frame");
}

// ---- onGloballyPositioned ----------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/OnGloballyPositionedTest.kt, onPositionedCalledWhenLayerChanged
#[test]
fn a_layer_s_change_alone_reports_the_new_position() {
    let offset = mutable_state_of(0.0f32);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let measures = Rc::new(Cell::new(0));
    let (o, s, m) = (offset.handle(), seen.clone(), measures.clone());
    let policy = measure_policy(move |scope, _, _| {
        m.set(m.get() + 1);
        scope.layout(100, 200)
    });
    let mut host = host(move || {
        let s = s.clone();
        Layout(
            Modifier
                .graphics_layer(move |layer| layer.translation_x = o.get())
                .on_globally_positioned(move |coordinates: LayoutCoordinates| {
                    s.borrow_mut().push(coordinates.position_in_root().x)
                }),
            policy.clone(),
            callback(|| {}),
        );
    });
    assert_eq!(*seen.borrow(), [0.0]);
    offset.set(5.0);
    host.frame();
    assert_eq!(*seen.borrow(), [0.0, 5.0]);
    assert_eq!(measures.get(), 1, "nothing was laid out");
}

#[test]
fn a_layer_s_change_reports_those_under_it_and_not_others() {
    let offset = mutable_state_of(0.0f32);
    let (under, next_to) = (Rc::new(RefCell::new(Vec::new())), Rc::new(Cell::new(0)));
    let (o, u, n) = (offset.handle(), under.clone(), next_to.clone());

    struct Counts(Rc<Cell<u32>>);
    impl ModifierNode for Counts {
        fn as_global_position_aware(&mut self) -> Option<&mut dyn GlobalPositionAwareModifierNode> {
            Some(self)
        }
    }
    impl GlobalPositionAwareModifierNode for Counts {
        fn on_globally_positioned(&mut self, _: LayoutCoordinates, _: &mut NodeContext<'_>) {
            self.0.set(self.0.get() + 1);
        }
    }
    struct CountsElement(Rc<Cell<u32>>);
    impl ModifierNodeElement for CountsElement {
        type Node = Counts;
        fn create(&self) -> Counts {
            Counts(self.0.clone())
        }
        fn update(&self, _: &mut Counts) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }

    let mut host = host(move || {
        let (u, n) = (u.clone(), n.clone());
        stack(Modifier, move || {
            let u = u.clone();
            stack(Modifier.graphics_layer(move |layer| layer.translation_y = o.get()), move || {
                let u = u.clone();
                Leaf(Modifier.size(px(5)).on_globally_positioned(
                    move |coordinates: LayoutCoordinates| {
                        u.borrow_mut().push(coordinates.position_in_root().y)
                    },
                ));
            });
            Leaf(Modifier.size(px(5)).then_custom(CountsElement(n.clone())));
        });
    });
    assert_eq!(*under.borrow(), [0.0]);
    let others = next_to.get();
    offset.set(3.0);
    host.frame();
    assert_eq!(*under.borrow(), [0.0, 3.0]);
    assert_eq!(next_to.get(), others, "not under that layer");
}

#[test]
fn a_layer_s_alpha_alone_reports_nothing() {
    let alpha = mutable_state_of(1.0f32);
    let told = Rc::new(Cell::new(0));
    let (a, t) = (alpha.handle(), told.clone());

    struct Counts(Rc<Cell<u32>>);
    impl ModifierNode for Counts {
        fn as_global_position_aware(&mut self) -> Option<&mut dyn GlobalPositionAwareModifierNode> {
            Some(self)
        }
    }
    impl GlobalPositionAwareModifierNode for Counts {
        fn on_globally_positioned(&mut self, _: LayoutCoordinates, _: &mut NodeContext<'_>) {
            self.0.set(self.0.get() + 1);
        }
    }
    struct CountsElement(Rc<Cell<u32>>);
    impl ModifierNodeElement for CountsElement {
        type Node = Counts;
        fn create(&self) -> Counts {
            Counts(self.0.clone())
        }
        fn update(&self, _: &mut Counts) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }

    let mut host = host(move || {
        Leaf(
            Modifier
                .graphics_layer(move |layer| layer.alpha = a.get())
                .then_custom(CountsElement(t.clone()))
                .size(px(5)),
        );
    });
    let before = told.get();
    assert!(before > 0);
    alpha.set(0.5);
    host.frame();
    assert_eq!(told.get(), before);
}

#[test]
fn an_inner_layer_s_change_reports_too() {
    let offset = mutable_state_of(0.0f32);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (o, s) = (offset.handle(), seen.clone());
    let mut host = host(move || {
        let s = s.clone();
        Leaf(
            Modifier
                .draw_behind(|_| {})
                .graphics_layer(move |layer| layer.translation_x = o.get())
                .on_globally_positioned(move |coordinates: LayoutCoordinates| {
                    s.borrow_mut().push(coordinates.position_in_root().x)
                })
                .size(px(5)),
        );
    });
    offset.set(2.0);
    host.frame();
    assert_eq!(*seen.borrow(), [0.0, 2.0]);
}

#[test]
fn a_position_written_back_by_its_callback_is_drawn_in_the_same_frame() {
    let offset = mutable_state_of(0.0f32);
    let written = mutable_state_of(0.0f32);
    let (o, w) = (offset.handle(), written.handle());
    let mut host = host(move || {
        Leaf(
            Modifier
                .graphics_layer(move |layer| layer.translation_x = o.get())
                .on_globally_positioned(move |coordinates: LayoutCoordinates| {
                    w.set(coordinates.position_in_root().x)
                })
                .size(px(5)),
        );
    });
    offset.set(4.0);
    host.frame();
    assert_eq!(written.get(), 4.0);
}

#[test]
fn a_position_aware_node_gets_a_context() {
    let observer_free = Rc::new(Cell::new(None));

    struct WithContext(Rc<Cell<Option<Density>>>);
    impl ModifierNode for WithContext {
        fn as_global_position_aware(&mut self) -> Option<&mut dyn GlobalPositionAwareModifierNode> {
            Some(self)
        }
    }
    impl GlobalPositionAwareModifierNode for WithContext {
        fn on_globally_positioned(&mut self, _: LayoutCoordinates, context: &mut NodeContext<'_>) {
            self.0.set(Some(context.require_density()));
        }
    }
    struct WithContextElement(Rc<Cell<Option<Density>>>);
    impl ModifierNodeElement for WithContextElement {
        type Node = WithContext;
        fn create(&self) -> WithContext {
            WithContext(self.0.clone())
        }
        fn update(&self, _: &mut WithContext) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }

    let o = observer_free.clone();
    let _host = host(move || Leaf(Modifier.then_custom(WithContextElement(o.clone())).size(px(1))));
    assert!(observer_free.get().is_some());
}

// ---- Modifier.on_size_changed on the node --------------------------------------------------------

#[test]
fn on_size_changed_adds_no_layer() {
    let handle: Rc<Cell<Option<NodeHandle>>> = Rc::default();
    let h = handle.clone();
    let _host = host(move || {
        Leaf(
            Modifier
                .on_size_changed(|_: IntSize| {})
                .then_custom(OwnerElement(Vec::new(), h.clone()))
                .size(px(1)),
        );
    });
    let (layer, layers) = handle.get().unwrap().layer_for_test().unwrap();
    assert_eq!((layer, layers), (0, 2), "the size modifier's layer, and its policy's");
}
