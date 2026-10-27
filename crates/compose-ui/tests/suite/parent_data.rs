//! Custom parent data (`ParentDataModifierNode`) and `Modifier.layout_id`
//! (docs/parity/design/modifier-nodes.md §3.10, step MN-10).

use crate::support;

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::*;
use support::*;

type Seen = Rc<RefCell<Vec<Option<Rc<dyn Any>>>>>;

/// A policy recording its children's parent data, then measuring and placing them.
fn recording(seen: &Seen) -> Rc<dyn MeasurePolicy> {
    let seen = seen.clone();
    measure_policy(move |scope, measurables, constraints| {
        let mut seen = seen.borrow_mut();
        seen.clear();
        for &measurable in measurables {
            seen.push(scope.parent_data_any(measurable));
            let placeable = scope.measure(measurable, constraints);
            scope.place(&placeable, 0, 0);
        }
        scope.layout(0, 0)
    })
}

/// As the Compose test's `SimpleDrawChild`: a drawn 10 × 10 leaf.
fn simple_draw_child() {
    Leaf(Modifier.size(10.dp()).draw_behind(|scope| {
        let bounds = scope.bounds();
        scope.draw_rect(bounds, Paint::fill(Color::from_argb(0xFF00_8000)));
    }));
}

fn id_of(data: &Option<Rc<dyn Any>>) -> Option<Rc<dyn Any>> {
    let data = data.as_ref()?;
    (**data).downcast_ref::<LayoutIdParentData>().map(|data| data.layout_id.clone())
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/ParentDataModifierTest.kt, parentDataDefaultsToNull
#[test]
fn parent_data_defaults_to_none() {
    let seen = Seen::default();
    let policy = recording(&seen);
    let _host = host(move || Layout(Modifier, policy.clone(), callback(simple_draw_child)));
    let seen = seen.borrow();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].is_none());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/ParentDataModifierTest.kt, parentDataIsReset
#[test]
fn a_layout_s_own_parent_data_doesn_t_reach_its_children() {
    let seen = Seen::default();
    let policy = recording(&seen);
    let _host = host(move || {
        Layout(Modifier.layout_id("Hello"), policy.clone(), callback(simple_draw_child));
    });
    assert!(seen.borrow()[0].is_none());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/ParentDataModifierTest.kt, multiChildLayoutTest_doesNotOverrideChildrenParentData
#[test]
fn each_child_keeps_its_own_parent_data() {
    let seen = Seen::default();
    let policy = recording(&seen);
    let _host = host(move || {
        Layout(
            Modifier,
            policy.clone(),
            callback(|| {
                Leaf(Modifier.layout_id(0));
                Leaf(Modifier.layout_id(1));
            }),
        );
    });
    let seen = seen.borrow();
    let ids: Vec<Option<i32>> = seen
        .iter()
        .map(|data| id_of(data).and_then(|id| id.downcast_ref::<i32>().copied()))
        .collect();
    assert_eq!(ids, [Some(0), Some(1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/ParentDataModifierTest.kt, parentDataOnPlaceable
#[test]
fn a_placeable_has_its_measurable_s_parent_data() {
    let id = Rc::new(RefCell::new(None));
    let seen = id.clone();
    let policy = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        let data = scope.parent_data_any(placeable);
        *seen.borrow_mut() = id_of(&data).and_then(|id| id.downcast_ref::<&str>().copied());
        scope.layout(0, 0)
    });
    let _host = host(move || {
        Layout(Modifier, policy.clone(), callback(|| Leaf(Modifier.layout_id("data"))));
    });
    assert_eq!(*id.borrow(), Some("data"));
}

/// A node that lays out and gives parent data, as the Compose test's
/// `ParentDataAndLayoutNode`.
struct ParentDataAndLayout(&'static str);

struct ParentDataAndLayoutNode(&'static str);

impl ModifierNodeElement for ParentDataAndLayout {
    type Node = ParentDataAndLayoutNode;
    fn create(&self) -> ParentDataAndLayoutNode {
        ParentDataAndLayoutNode(self.0)
    }
    fn update(&self, node: &mut ParentDataAndLayoutNode) {
        node.0 = self.0;
    }
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl ModifierNode for ParentDataAndLayoutNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn as_parent_data(&self) -> Option<&dyn ParentDataModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for ParentDataAndLayoutNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(content, constraints);
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }
}

impl ParentDataModifierNode for ParentDataAndLayoutNode {
    fn modify_parent_data(&self, _: Density, _: Option<Rc<dyn Any>>) -> Option<Rc<dyn Any>> {
        Some(Rc::new(self.0))
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/ParentDataModifierTest.kt, implementingBothParentDataAndLayoutModifier
#[test]
fn a_node_can_lay_out_and_give_parent_data() {
    let seen = Seen::default();
    let policy = recording(&seen);
    let _host = host(move || {
        Layout(
            Modifier,
            policy.clone(),
            callback(|| Leaf(Modifier.then_custom(ParentDataAndLayout("data")))),
        );
    });
    let seen = seen.borrow();
    let data = seen[0].as_ref().expect("parent data");
    assert_eq!((**data).downcast_ref::<&str>(), Some(&"data"));
}

/// As the Compose test's `ParentInt`: its value as parent data.
struct ParentInt(i32);

struct ParentIntNode(i32);

impl ModifierNodeElement for ParentInt {
    type Node = ParentIntNode;
    fn create(&self) -> ParentIntNode {
        ParentIntNode(self.0)
    }
    fn update(&self, node: &mut ParentIntNode) {
        node.0 = self.0;
    }
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl ModifierNode for ParentIntNode {
    fn as_parent_data(&self) -> Option<&dyn ParentDataModifierNode> {
        Some(self)
    }
}

impl ParentDataModifierNode for ParentIntNode {
    fn modify_parent_data(&self, _: Density, _: Option<Rc<dyn Any>>) -> Option<Rc<dyn Any>> {
        Some(Rc::new(self.0))
    }
}

/// A policy as large as its child's `ParentInt`.
fn sized_by_parent_data(measured: &Rc<Cell<i32>>) -> Rc<dyn MeasurePolicy> {
    let measured = measured.clone();
    measure_policy(move |scope, measurables, constraints| {
        let data = scope.parent_data_any(measurables[0]).expect("parent data");
        let size = *(*data).downcast_ref::<i32>().expect("an i32");
        let placeable = scope.measure(measurables[0], constraints);
        scope.place(&placeable, 0, 0);
        measured.set(size);
        scope.layout(size, size)
    })
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/ParentDataModifierTest.kt, remeasureOnParentDataChanged
#[test]
fn a_parent_data_change_measures_the_parent_again() {
    let size = mutable_state_of(10);
    let measured = Rc::new(Cell::new(0));
    let (s, policy) = (size.handle(), sized_by_parent_data(&measured));
    let mut host = host(move || {
        Layout(
            Modifier,
            policy.clone(),
            callback(move || Leaf(Modifier.then_custom(ParentInt(s.get())))),
        );
    });
    assert_eq!(measured.get(), 10);
    size.set(20);
    host.frame();
    assert_eq!(measured.get(), 20);
}

/// The outermost parent data node has the last word: nodes fold from the tail to the head, each
/// given what those after it gave, as Compose's `modifyParentData` chain.
#[test]
fn parent_data_folds_from_the_tail_to_the_head() {
    struct Add(i32);
    struct AddNode(i32);
    impl ModifierNodeElement for Add {
        type Node = AddNode;
        fn create(&self) -> AddNode {
            AddNode(self.0)
        }
        fn update(&self, node: &mut AddNode) {
            node.0 = self.0;
        }
        fn same(&self, other: &Self) -> bool {
            self.0 == other.0
        }
    }
    impl ModifierNode for AddNode {
        fn as_parent_data(&self) -> Option<&dyn ParentDataModifierNode> {
            Some(self)
        }
    }
    impl ParentDataModifierNode for AddNode {
        fn modify_parent_data(&self, _: Density, data: Option<Rc<dyn Any>>) -> Option<Rc<dyn Any>> {
            let before = data.and_then(|data| (*data).downcast_ref::<Vec<i32>>().cloned());
            let mut values = before.unwrap_or_default();
            values.push(self.0);
            Some(Rc::new(values))
        }
    }
    let seen = Seen::default();
    let policy = recording(&seen);
    let _host = host(move || {
        let child = || Leaf(Modifier.then_custom(Add(1)).padding(2.dp()).then_custom(Add(2)));
        Layout(Modifier, policy.clone(), callback(child));
    });
    let seen = seen.borrow();
    let data = seen[0].as_ref().expect("parent data");
    assert_eq!((**data).downcast_ref::<Vec<i32>>(), Some(&vec![2, 1]));
}

/// A layout modifier's content has its layout node's parent data, the whole chain's, as each of
/// Compose's coordinators folds the whole chain (`NodeCoordinator.parentData`).
#[test]
fn a_layout_modifier_s_content_has_its_layout_node_s_parent_data() {
    let inner = Rc::new(RefCell::new(None));
    let seen = inner.clone();
    let _host = host(move || {
        let seen = seen.clone();
        let modifier = Modifier
            .layout_id("outer")
            .layout(move |scope, content, constraints| {
                let data = scope.parent_data_any(content);
                *seen.borrow_mut() =
                    Some(id_of(&data).and_then(|id| id.downcast_ref::<&str>().copied()));
                let placeable = scope.measure(content, constraints);
                scope.place(&placeable, 0, 0);
                scope.layout(placeable.width, placeable.height)
            })
            .layout_id("inner")
            .size(10.dp());
        Leaf(modifier);
    });
    assert_eq!(*inner.borrow(), Some(Some("outer")), "the outermost has the last word");
}

/// A node changing its parent data outside an update says so (`invalidate_parent_data`): the
/// parent measures again.
#[test]
fn invalidating_parent_data_measures_the_parent_again() {
    type Shared = Rc<Cell<i32>>;
    struct Live(Shared, Rc<RefCell<Option<NodeHandle>>>);
    struct LiveNode(Shared, Rc<RefCell<Option<NodeHandle>>>);
    impl ModifierNodeElement for Live {
        type Node = LiveNode;
        fn create(&self) -> LiveNode {
            LiveNode(self.0.clone(), self.1.clone())
        }
        fn update(&self, _: &mut LiveNode) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    impl ModifierNode for LiveNode {
        fn as_parent_data(&self) -> Option<&dyn ParentDataModifierNode> {
            Some(self)
        }
        fn on_attach(&mut self, context: &mut NodeContext<'_>) {
            *self.1.borrow_mut() = Some(context.handle());
        }
    }
    impl ParentDataModifierNode for LiveNode {
        fn modify_parent_data(&self, _: Density, _: Option<Rc<dyn Any>>) -> Option<Rc<dyn Any>> {
            Some(Rc::new(self.0.get()))
        }
    }
    let value = Rc::new(Cell::new(10));
    let handle = Rc::new(RefCell::new(None));
    let measured = Rc::new(Cell::new(0));
    let (v, h, policy) = (value.clone(), handle.clone(), sized_by_parent_data(&measured));
    let mut host = host(move || {
        let (v, h) = (v.clone(), h.clone());
        Layout(
            Modifier,
            policy.clone(),
            callback(move || Leaf(Modifier.then_custom(Live(v.clone(), h.clone())))),
        );
    });
    assert_eq!(measured.get(), 10);
    value.set(30);
    let node = handle.borrow().expect("a handle");
    node.invalidate_parent_data();
    host.frame();
    assert_eq!(measured.get(), 30);
    // From the node's own code (a task reaching it): its data, given once it's back in its
    // chain, is the new one.
    let changed = value.clone();
    compose_runtime::CoroutineScope::new().launch(async move {
        node.with_node(|_: &mut LiveNode, context| {
            changed.set(40);
            context.invalidate_parent_data();
        });
    });
    host.frame();
    host.frame();
    assert_eq!(measured.get(), 40);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/LayoutIdTest.kt, testTags
#[test]
fn layout_ids_reach_the_parent_s_measurables() {
    let ids = Rc::new(RefCell::new(Vec::new()));
    let seen = ids.clone();
    let policy = measure_policy(move |scope, measurables, _| {
        *seen.borrow_mut() = measurables
            .iter()
            .map(|&measurable| {
                scope.layout_id(measurable).and_then(|id| id.downcast_ref::<&str>().copied())
            })
            .collect();
        scope.layout(0, 0)
    });
    let _host = host(move || {
        Layout(
            Modifier,
            policy.clone(),
            callback(|| {
                Leaf(Modifier.layout_id("first"));
                Stack(Modifier.layout_id("second"), callback(|| Leaf(Modifier)));
                Stack(Modifier.layout_id("third"), callback(|| Leaf(Modifier)));
            }),
        );
    });
    assert_eq!(*ids.borrow(), [Some("first"), Some("second"), Some("third")]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/platform/LayoutIdTest.kt, testInspectableValue
#[test]
fn a_layout_id_shows_its_name_and_value() {
    assert_eq!(format!("{:?}", Modifier.layout_id("box")), "Modifier.layoutId(\"box\")");
}
