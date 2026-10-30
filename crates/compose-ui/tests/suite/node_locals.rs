//! Modifier nodes reading composition locals, and per-node density
//! (docs/parity/design/modifier-nodes.md §3.9, step MN-8).

use crate::support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::platform::UiHost;
use compose_ui::*;
use support::*;

static LOCAL_STATIC_INT: CompositionLocal<i32> =
    CompositionLocal::new_static("LocalStaticInt", || 0);
static LOCAL_INT: CompositionLocal<i32> = CompositionLocal::new("LocalInt", || 0);
static LOCAL_SOME_VALUE: CompositionLocal<i32> = CompositionLocal::new("LocalSomeValue", || -1);

type Seen = Rc<Cell<i32>>;

/// Reads a local where it draws, as the test's `DrawModifierNode` nodes.
#[derive(Clone)]
struct DrawReader(&'static CompositionLocal<i32>, Seen);
struct DrawReaderNode(&'static CompositionLocal<i32>, Seen);

impl ModifierNodeElement for DrawReader {
    type Node = DrawReaderNode;
    fn create(&self) -> DrawReaderNode {
        DrawReaderNode(self.0, self.1.clone())
    }
    fn update(&self, _: &mut DrawReaderNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for DrawReaderNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn is_composition_local_consumer(&self) -> bool {
        true
    }
}

impl DrawModifierNode for DrawReaderNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        self.1.set(scope.current_value_of(self.0));
    }
}

/// Reads a local where it measures.
struct LayoutReader(&'static CompositionLocal<i32>, Seen);
struct LayoutReaderNode(&'static CompositionLocal<i32>, Seen);

impl ModifierNodeElement for LayoutReader {
    type Node = LayoutReaderNode;
    fn create(&self) -> LayoutReaderNode {
        LayoutReaderNode(self.0, self.1.clone())
    }
    fn update(&self, _: &mut LayoutReaderNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for LayoutReaderNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn is_composition_local_consumer(&self) -> bool {
        true
    }
}

impl LayoutModifierNode for LayoutReaderNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        self.1.set(scope.current_value_of(self.0));
        let placeable = scope.measure(content, constraints);
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }
}

/// Reads a local in its `on_attach` and observes it, as the test's `ConsumeInAttachNode`.
struct AttachReader(&'static CompositionLocal<i32>, Seen);
struct AttachReaderNode(&'static CompositionLocal<i32>, Seen);

impl ModifierNodeElement for AttachReader {
    type Node = AttachReaderNode;
    fn create(&self) -> AttachReaderNode {
        AttachReaderNode(self.0, self.1.clone())
    }
    fn update(&self, _: &mut AttachReaderNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl AttachReaderNode {
    fn read(&mut self, context: &mut NodeContext<'_>) {
        let (local, seen) = (self.0, self.1.clone());
        let locals = context.composition_locals();
        context.observe_reads(move || seen.set(locals.get(local)));
    }
}

impl ModifierNode for AttachReaderNode {
    fn as_observer(&mut self) -> Option<&mut dyn ObserverModifierNode> {
        Some(self)
    }
    fn is_composition_local_consumer(&self) -> bool {
        true
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        assert_eq!(context.current_value_of(self.0), context.composition_locals().get(self.0));
        self.read(context);
    }
    fn on_composition_locals_changed(&mut self, context: &mut NodeContext<'_>) {
        self.read(context);
    }
}

impl ObserverModifierNode for AttachReaderNode {
    fn on_observed_reads_changed(&mut self, context: &mut NodeContext<'_>) {
        self.read(context);
    }
}

fn draw_node(local: &'static CompositionLocal<i32>, seen: &Seen) -> Modifier {
    Modifier.size(10.dp()).then_custom(DrawReader(local, seen.clone()))
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/CompositionLocalConsumerModifierNodeTest.kt, defaultValueReturnedIfNotProvided
#[test]
fn a_node_reads_a_local_s_default_when_none_is_provided() {
    let seen = Seen::new(Cell::new(-1));
    let s = seen.clone();
    let _host = host(move || Leaf(draw_node(&LOCAL_INT, &s)));
    assert_eq!(seen.get(), 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/CompositionLocalConsumerModifierNodeTest.kt, providedValueReturned
#[test]
fn a_node_reads_what_s_provided_where_it_is() {
    let seen = Seen::new(Cell::new(-1));
    let s = seen.clone();
    let _host = host(move || {
        let s = s.clone();
        CompositionLocalProvider(LOCAL_INT.provides(2), move || Leaf(draw_node(&LOCAL_INT, &s)));
    });
    assert_eq!(seen.get(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/CompositionLocalConsumerModifierNodeTest.kt, providedValueUpdatedReadsNewValue
#[test]
fn a_new_value_of_a_dynamic_local_draws_the_node_that_read_it() {
    let seen = Seen::new(Cell::new(-1));
    let provided = mutable_state_of(2);
    let (s, p) = (seen.clone(), provided.handle());
    let mut host = host(move || {
        let s = s.clone();
        CompositionLocalProvider(LOCAL_INT.provides(p.get()), move || {
            Leaf(draw_node(&LOCAL_INT, &s))
        });
    });
    assert_eq!(seen.get(), 2);
    provided.set(3);
    host.frame();
    assert_eq!(seen.get(), 3);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/CompositionLocalConsumerModifierNodeTest.kt, defaultStaticValueReturnedIfNotProvided
#[test]
fn a_node_reads_a_static_local_s_default_when_none_is_provided() {
    let seen = Seen::new(Cell::new(-1));
    let s = seen.clone();
    let _host = host(move || Leaf(draw_node(&LOCAL_STATIC_INT, &s)));
    assert_eq!(seen.get(), 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/CompositionLocalConsumerModifierNodeTest.kt, providedStaticValueReturned
#[test]
fn a_node_reads_a_provided_static_local() {
    let seen = Seen::new(Cell::new(-1));
    let s = seen.clone();
    let _host = host(move || {
        let s = s.clone();
        CompositionLocalProvider(LOCAL_STATIC_INT.provides(2), move || {
            Leaf(draw_node(&LOCAL_STATIC_INT, &s))
        });
    });
    assert_eq!(seen.get(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/CompositionLocalConsumerModifierNodeTest.kt, providedStaticValueUpdatedReadsNewValue
#[test]
fn a_new_value_of_a_static_local_draws_a_consumer_that_read_it() {
    let seen = Seen::new(Cell::new(-1));
    let provided = mutable_state_of(2);
    let (s, p) = (seen.clone(), provided.handle());
    let mut host = host(move || {
        let s = s.clone();
        CompositionLocalProvider(LOCAL_STATIC_INT.provides(p.get()), move || {
            Leaf(draw_node(&LOCAL_STATIC_INT, &s))
        });
    });
    assert_eq!(seen.get(), 2);
    provided.set(3);
    host.frame();
    assert_eq!(seen.get(), 3);
}

fn moved_content_case(local: &'static CompositionLocal<i32>) {
    let seen = Seen::new(Cell::new(-1));
    let provided = mutable_state_of(42);
    let content_key = mutable_state_of(1);
    let (s, p, k) = (seen.clone(), provided.handle(), content_key.handle());
    let mut host = host(move || {
        let s = s.clone();
        CompositionLocalProvider(local.provides(p.get()), move || {
            let s = s.clone();
            ReusableContent(k.get(), move || Leaf(draw_node(local, &s)));
        });
    });
    assert_eq!(seen.get(), 42);
    content_key.set(2);
    provided.set(86);
    host.frame();
    assert_eq!(seen.get(), 86);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/CompositionLocalConsumerModifierNodeTest.kt, compositionLocalsUpdateWhenContentMoves
#[test]
fn a_dynamic_local_is_read_anew_when_content_moves() {
    moved_content_case(&LOCAL_INT);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/CompositionLocalConsumerModifierNodeTest.kt, staticCompositionLocalsUpdateWhenContentMoves
#[test]
fn a_static_local_is_read_anew_when_content_moves() {
    moved_content_case(&LOCAL_STATIC_INT);
}

/// `provided`'s value of `LOCAL_SOME_VALUE`, around a 10 × 10 leaf with `node`.
fn injection_host(provided: MutableState<i32>, node: impl Fn() -> Modifier + 'static) -> UiHost {
    let node = Rc::new(node);
    host(move || {
        let node = node.clone();
        CompositionLocalProvider(LOCAL_SOME_VALUE.provides(provided.get()), move || {
            Leaf(node().size(10.dp()));
        });
    })
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/CompositionLocalMapInjectionTest.kt, consumeInDrawGetsNotifiedOfChanges
#[test]
fn a_local_read_in_draw_gets_its_new_value() {
    let seen = Seen::new(Cell::new(0));
    let state = mutable_state_of(1);
    let s = seen.clone();
    let mut host = injection_host(state.handle(), move || {
        Modifier.then_custom(DrawReader(&LOCAL_SOME_VALUE, s.clone()))
    });
    assert_eq!(seen.get(), 1);
    state.set(2);
    host.frame();
    assert_eq!(seen.get(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/CompositionLocalMapInjectionTest.kt, consumeInLayoutGetsNotifiedOfChanges
#[test]
fn a_local_read_in_layout_gets_its_new_value() {
    let seen = Seen::new(Cell::new(0));
    let state = mutable_state_of(1);
    let s = seen.clone();
    let mut host = injection_host(state.handle(), move || {
        Modifier.then_custom(LayoutReader(&LOCAL_SOME_VALUE, s.clone()))
    });
    assert_eq!(seen.get(), 1);
    state.set(2);
    host.frame();
    assert_eq!(seen.get(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/CompositionLocalMapInjectionTest.kt, consumeInAttachGetsNotifiedOfChanges
#[test]
fn a_local_read_in_attach_and_observed_gets_its_new_value() {
    let seen = Seen::new(Cell::new(0));
    let state = mutable_state_of(1);
    let s = seen.clone();
    let mut host = injection_host(state.handle(), move || {
        Modifier.then_custom(AttachReader(&LOCAL_SOME_VALUE, s.clone()))
    });
    assert_eq!(seen.get(), 1);
    state.set(2);
    host.frame();
    assert_eq!(seen.get(), 2);
}

/// Panics reading a local detached.
struct Detached(Rc<RefCell<Option<NodeHandle>>>);
struct DetachedNode(Rc<RefCell<Option<NodeHandle>>>);

impl ModifierNodeElement for Detached {
    type Node = DetachedNode;
    fn create(&self) -> DetachedNode {
        DetachedNode(self.0.clone())
    }
    fn update(&self, _: &mut DetachedNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for DetachedNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        *self.0.borrow_mut() = Some(context.handle());
    }
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/CompositionLocalConsumerModifierNode.kt, currentValueOf
#[test]
#[should_panic(expected = "while the node isn't attached")]
fn reading_a_local_detached_panics() {
    let active = mutable_state_of(true);
    let held = Rc::new(RefCell::new(None));
    let (a, h) = (active.handle(), held.clone());
    let mut host = host(move || {
        let h = h.clone();
        ReusableContentHost(a.get(), move || {
            let h = h.clone();
            ReusableContent(0, move || Leaf(Modifier.then_custom(Detached(h.clone()))));
        });
    });
    active.set(false);
    host.frame();
    let node = held.borrow().expect("attached once");
    node.with_node(|_: &mut DetachedNode, context| context.current_value_of(&LOCAL_INT));
}

// ---- Per-node density --------------------------------------------------------------------------

fn with_density(density: MutableState<Density>, content: impl Fn() + 'static) -> UiHost {
    let content = Rc::new(content);
    host(move || {
        let content = content.clone();
        CompositionLocalProvider(LOCAL_DENSITY.provides(density.get()), move || content());
    })
}

/// A provider group's id comes back for another provider: what the last node resolved under the
/// first isn't the second's.
#[test]
fn a_new_provider_in_a_recycled_group_gives_its_own_density() {
    let which = mutable_state_of(0);
    let seen = Rc::new(Cell::new(Density::default()));
    let (w, s) = (which.handle(), seen.clone());
    let policy = measure_policy(move |scope, _, _| {
        s.set(scope.density());
        scope.layout(0, 0)
    });
    let first = Density { density: 2.0, font_scale: 1.0 };
    let second = Density { density: 3.0, font_scale: 1.0 };
    let mut host = host(move || {
        let p = policy.clone();
        match w.get() {
            0 => CompositionLocalProvider(LOCAL_DENSITY.provides(first), move || {
                Layout(Modifier, p.clone(), callback(|| {}));
            }),
            1 => {}
            _ => CompositionLocalProvider(LOCAL_DENSITY.provides(second), move || {
                Layout(Modifier, p.clone(), callback(|| {}));
            }),
        }
    });
    assert_eq!(seen.get(), first);
    which.set(1);
    host.frame();
    which.set(2);
    host.frame();
    assert_eq!(seen.get(), second, "by way of nothing");
    which.set(0);
    host.frame();
    assert_eq!(seen.get(), first, "at once");
}

/// What a node resolved is shared with the next only where a provider between them gives none
/// of it.
#[test]
fn nodes_under_providers_next_to_each_other_get_their_own_density() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    let policy = measure_policy(move |scope, _, _| {
        s.borrow_mut().push(scope.density().density);
        scope.layout(0, 0)
    });
    let density = |density| Density { density, font_scale: 1.0 };
    let _host = host(move || {
        let leaf = {
            let policy = policy.clone();
            Rc::new(move || Layout(Modifier, policy.clone(), callback(|| {})))
        };
        let around = density(2.0);
        let (l1, l2, l3, l4, l5, l6) =
            (leaf.clone(), leaf.clone(), leaf.clone(), leaf.clone(), leaf.clone(), leaf.clone());
        CompositionLocalProvider(LOCAL_DENSITY.provides(around), move || {
            // Under it; next to it, giving something else; next to that, giving a density.
            (l1)();
            let l2 = l2.clone();
            CompositionLocalProvider(LOCAL_INT.provides(1), move || (l2)());
            let l3 = l3.clone();
            CompositionLocalProvider(LOCAL_DENSITY.provides(density(3.0)), move || (l3)());
            // Next to one that gave a density, giving something else; then back around them.
            let l4 = l4.clone();
            CompositionLocalProvider(LOCAL_INT.provides(2), move || (l4)());
            (l5)();
            // Inside one giving something else, one giving a density.
            let l6 = l6.clone();
            CompositionLocalProvider(LOCAL_INT.provides(3), move || {
                let l6 = l6.clone();
                CompositionLocalProvider(LOCAL_DENSITY.provides(density(4.0)), move || (l6)());
            });
        });
    });
    assert_eq!(*seen.borrow(), [2.0, 2.0, 3.0, 2.0, 2.0, 4.0]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LayoutNodeDensityTest.kt, compositionLocalDensityChangeTriggersRemeasure
#[test]
fn a_new_local_density_measures_with_it() {
    let density = mutable_state_of(Density { density: 5.0, font_scale: 7.0 });
    let seen = Rc::new(Cell::new(Density::default()));
    let s = seen.clone();
    let policy = measure_policy(move |scope, _, _| {
        s.set(scope.density());
        scope.layout(0, 0)
    });
    let mut host = with_density(density.handle(), move || {
        Layout(Modifier, policy.clone(), callback(|| {}));
    });
    assert_eq!(seen.get(), Density { density: 5.0, font_scale: 7.0 });
    density.set(Density { density: 9.0, font_scale: 11.0 });
    host.frame();
    assert_eq!(seen.get(), Density { density: 9.0, font_scale: 11.0 });
}

fn redraw_case(modifier: impl Fn(Modifier) -> Modifier + 'static) {
    let density = mutable_state_of(Density { density: 5.0, font_scale: 7.0 });
    let seen = Rc::new(Cell::new(Density::default()));
    let s = seen.clone();
    let mut host = with_density(density.handle(), move || {
        let s = s.clone();
        Leaf(modifier(Modifier.size(10.dp()).draw_behind(move |scope| s.set(scope.density()))));
    });
    assert_eq!(seen.get(), Density { density: 5.0, font_scale: 7.0 });
    density.set(Density { density: 9.0, font_scale: 11.0 });
    host.frame();
    assert_eq!(seen.get(), Density { density: 9.0, font_scale: 11.0 });
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LayoutNodeDensityTest.kt, compositionLocalDensityChangeTriggersRedraw
#[test]
fn a_new_local_density_draws_with_it() {
    redraw_case(|modifier| modifier);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LayoutNodeDensityTest.kt, compositionLocalDensityChangeTriggersRedrawLayerAfterDraw
#[test]
fn a_new_local_density_draws_with_it_under_a_later_layer() {
    redraw_case(|modifier| modifier.graphics_layer(|_| {}));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LayoutNodeDensityTest.kt, compositionLocalDensityChangeTriggersRedrawLayerBeforeDraw
#[test]
fn a_new_local_density_draws_with_it_inside_a_layer() {
    let density = mutable_state_of(Density { density: 5.0, font_scale: 7.0 });
    let seen = Rc::new(Cell::new(Density::default()));
    let s = seen.clone();
    let mut host = with_density(density.handle(), move || {
        let s = s.clone();
        Leaf(
            Modifier
                .size(10.dp())
                .graphics_layer(|_| {})
                .draw_behind(move |scope| s.set(scope.density())),
        );
    });
    density.set(Density { density: 9.0, font_scale: 11.0 });
    host.frame();
    assert_eq!(seen.get(), Density { density: 9.0, font_scale: 11.0 });
}

/// Two subtrees at different densities.
#[test]
fn each_subtree_measures_at_its_own_density() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    let _host = host(move || {
        let (first, second) = (s.clone(), s.clone());
        let policy = move |seen: Rc<RefCell<Vec<f32>>>| {
            measure_policy(move |scope, _, _| {
                seen.borrow_mut().push(scope.density().density);
                scope.layout(0, 0)
            })
        };
        CompositionLocalProvider(LOCAL_DENSITY.provides(Density::new(2.0)), move || {
            Layout(Modifier, policy(first.clone()), callback(|| {}));
        });
        CompositionLocalProvider(LOCAL_DENSITY.provides(Density::new(3.0)), move || {
            Layout(Modifier, policy(second.clone()), callback(|| {}));
        });
    });
    assert_eq!(&*seen.borrow(), &[2.0, 3.0]);
}

/// Tells its density when attached and when it changes, as the test's node.
struct DensityLog(Rc<RefCell<Vec<f32>>>);
struct DensityLogNode(Rc<RefCell<Vec<f32>>>);

impl ModifierNodeElement for DensityLog {
    type Node = DensityLogNode;
    fn create(&self) -> DensityLogNode {
        DensityLogNode(self.0.clone())
    }
    fn update(&self, _: &mut DensityLogNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for DensityLogNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.0.borrow_mut().push(context.require_density().density);
    }
    fn on_density_change(&mut self, context: &mut NodeContext<'_>) {
        self.0.borrow_mut().push(context.require_density().density);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeOnDensityChangeTest.kt, densityChange_triggersNodeCallback
#[test]
fn a_new_local_density_tells_the_node_below_it_alone() {
    let density = mutable_state_of(Density::new(1.0));
    let (inside, outside) = (Rc::new(RefCell::new(Vec::new())), Rc::new(RefCell::new(Vec::new())));
    let (i, o, d) = (inside.clone(), outside.clone(), density.handle());
    let mut host = host(move || {
        Leaf(Modifier.then_custom(DensityLog(o.clone())));
        let i = i.clone();
        CompositionLocalProvider(LOCAL_DENSITY.provides(d.get()), move || {
            Leaf(Modifier.then_custom(DensityLog(i.clone())));
        });
    });
    assert_eq!(&*inside.borrow(), &[1.0]);
    density.set(Density::new(3.0));
    host.frame();
    assert_eq!(&*inside.borrow(), &[1.0, 3.0]);
    assert_eq!(&*outside.borrow(), &[1.0], "outside the provider, nothing changes");
}

struct CancelLog(Rc<Cell<u32>>);
struct CancelLogNode(Rc<Cell<u32>>);

impl ModifierNodeElement for CancelLog {
    type Node = CancelLogNode;
    fn create(&self) -> CancelLogNode {
        CancelLogNode(self.0.clone())
    }
    fn update(&self, _: &mut CancelLogNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for CancelLogNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for CancelLogNode {
    fn on_pointer_event(
        &mut self,
        _: &mut PointerEvent,
        _: PointerEventPass,
        _: IntSize,
        _: &mut EventContext<'_>,
    ) {
    }
    fn on_cancel_pointer_input(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeOnDensityChangeTest.kt, densityChange_backwardsCompatNode_pointerInputModifier_triggersOnCancel
#[test]
fn a_new_local_density_cancels_a_pointer_input_node() {
    let cancels = Rc::new(Cell::new(0));
    let density = mutable_state_of(Density::new(1.0));
    let c = cancels.clone();
    let mut host = with_density(density.handle(), move || {
        Leaf(Modifier.then_custom(CancelLog(c.clone())));
    });
    assert_eq!(cancels.get(), 0);
    density.set(Density::new(3.0));
    host.frame();
    assert_eq!(cancels.get(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeOnDensityChangeTest.kt, densityChange_backwardsCompatNode_nonPointerInputModifier_doesNotCrash
#[test]
fn a_new_local_density_with_a_draw_node_only_is_fine() {
    let density = mutable_state_of(Density::new(1.0));
    let seen = Seen::new(Cell::new(0));
    let s = seen.clone();
    let mut host = with_density(density.handle(), move || Leaf(draw_node(&LOCAL_INT, &s)));
    density.set(Density::new(3.0));
    host.frame();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/PointerInputDensityTest.kt, compositionLocalDensityChangeRestartsPointerInputOverload1
#[test]
fn a_new_local_density_restarts_a_pointer_input_block() {
    let density = mutable_state_of(Density::new(5.0));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    let mut host = with_density(density.handle(), move || {
        let s = s.clone();
        Leaf(Modifier.size(50.dp()).pointer_input((), move |scope| {
            let s = s.clone();
            async move {
                s.borrow_mut().push(scope.density().density);
                loop {
                    scope.await_pointer_event(PointerEventPass::Main).await;
                }
            }
        }));
    });
    let tap = |host: &mut UiHost| {
        host.send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
        host.send_pointer(PointerEvent::release(Offset::new(10.0, 10.0)));
        host.frame();
    };
    tap(&mut host);
    assert_eq!(&*seen.borrow(), &[5.0]);
    density.set(Density::new(9.0));
    host.frame();
    tap(&mut host);
    assert_eq!(&*seen.borrow(), &[5.0, 9.0]);
}

// ---- View configuration for a subtree (D907) ----------------------------------------------------

#[derive(Clone)]
struct ConfigurationLog(Rc<RefCell<Vec<u64>>>);
struct ConfigurationLogNode(Rc<RefCell<Vec<u64>>>);

impl ModifierNodeElement for ConfigurationLog {
    type Node = ConfigurationLogNode;
    fn create(&self) -> ConfigurationLogNode {
        ConfigurationLogNode(self.0.clone())
    }
    fn update(&self, _: &mut ConfigurationLogNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for ConfigurationLogNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
    fn on_view_configuration_change(&mut self, _: &mut NodeContext<'_>) {
        self.0.borrow_mut().push(0);
    }
}

impl PointerInputModifierNode for ConfigurationLogNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _: IntSize,
        context: &mut EventContext<'_>,
    ) {
        if pass == PointerEventPass::Main && event.kind == PointerEventType::Press {
            self.0.borrow_mut().push(context.view_configuration().long_press_timeout_millis);
        }
    }
}

fn long_press(millis: u64) -> ViewConfiguration {
    ViewConfiguration { long_press_timeout_millis: millis, ..ViewConfiguration::default() }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/gestures/SuspendingGestureTestUtil.kt, LocalViewConfiguration
#[test]
fn a_subtree_has_the_view_configuration_provided_to_it() {
    let inside = Rc::new(RefCell::new(Vec::new()));
    let outside = Rc::new(RefCell::new(Vec::new()));
    let (i, o) = (inside.clone(), outside.clone());
    let mut host = host(move || {
        let i = i.clone();
        let o = o.clone();
        Column!({
            Leaf(Modifier.size(20.dp()).then_custom(ConfigurationLog(o.clone())));
            let i = i.clone();
            CompositionLocalProvider(
                LOCAL_VIEW_CONFIGURATION.provides(long_press(1234)),
                move || {
                    Leaf(Modifier.size(20.dp()).then_custom(ConfigurationLog(i.clone())));
                },
            );
        });
    });
    host.send_pointer(PointerEvent::press(Offset::new(5.0, 5.0)));
    host.send_pointer(PointerEvent::release(Offset::new(5.0, 5.0)));
    host.send_pointer(PointerEvent::press(Offset::new(5.0, 25.0)));
    host.send_pointer(PointerEvent::release(Offset::new(5.0, 25.0)));
    assert_eq!(&*outside.borrow(), &[400], "the host's");
    assert_eq!(&*inside.borrow(), &[1234]);
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/LayoutNode.kt, viewConfiguration
#[test]
fn a_new_view_configuration_tells_the_nodes_below_it_and_cancels_their_gestures() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let cancels = Rc::new(Cell::new(0));
    let timeout = mutable_state_of(1000u64);
    let (l, c, t) = (log.clone(), cancels.clone(), timeout.handle());
    let mut host = host(move || {
        let (l, c) = (l.clone(), c.clone());
        CompositionLocalProvider(
            LOCAL_VIEW_CONFIGURATION.provides(long_press(t.get())),
            move || {
                Leaf(
                    Modifier
                        .size(20.dp())
                        .then_custom(ConfigurationLog(l.clone()))
                        .then_custom(CancelLog(c.clone())),
                );
            },
        );
    });
    timeout.set(2000);
    host.frame();
    assert_eq!(&*log.borrow(), &[0]);
    assert_eq!(cancels.get(), 1);
    host.send_pointer(PointerEvent::press(Offset::new(5.0, 5.0)));
    assert_eq!(&*log.borrow(), &[0, 2000]);
}

/// A subtree's own density and direction come from where it's composed, its parent's are its own.
#[test]
fn a_layout_direction_provided_reaches_the_nodes_below_it() {
    let seen = Rc::new(Cell::new(LayoutDirection::Ltr));
    let s = seen.clone();
    let _host = host(move || {
        let s = s.clone();
        CompositionLocalProvider(
            LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl),
            move || {
                let s = s.clone();
                let policy = measure_policy(move |scope, _, _| {
                    s.set(scope.layout_direction());
                    scope.layout(0, 0)
                });
                Layout(Modifier, policy, callback(|| {}));
            },
        );
    });
    assert_eq!(seen.get(), LayoutDirection::Rtl);
}

/// A shape recording the density it's asked at, as the Compose test's.
struct RecordingShape(Rc<Cell<f32>>);

impl CustomShape for RecordingShape {
    fn create_outline(&self, size: Size, _: LayoutDirection, density: &Density) -> Outline {
        self.0.set(density.density);
        Outline::Rectangle(Rect::from_offset_size(Offset::ZERO, size))
    }
    fn same(&self, other: &dyn CustomShape) -> bool {
        other.downcast_ref::<Self>().is_some()
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LayoutNodeDensityTest.kt, densityChangeRequestsLayerOutlineUpdate
#[test]
fn a_new_local_density_updates_a_layer_s_outline() {
    let density = mutable_state_of(Density::new(2.0));
    let seen = Rc::new(Cell::new(0.0));
    let s = seen.clone();
    let mut host = with_density(density.handle(), move || {
        let shape = Shape::Custom(Rc::new(RecordingShape(s.clone())));
        Leaf(Modifier.size(100.dp()).graphics_layer_with(GraphicsLayerScope {
            shape,
            clip: true,
            ..GraphicsLayerScope::default()
        }));
    });
    assert_eq!(seen.get(), 2.0);
    density.set(Density::new(2.5));
    host.frame();
    assert_eq!(seen.get(), 2.5);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/PointerInputViewConfigurationTest.kt, compositionLocalViewConfigurationChangeRestartsPointerInputOverload1
#[test]
fn a_new_view_configuration_restarts_a_pointer_input_block() {
    let timeout = mutable_state_of(1000u64);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let (s, t) = (seen.clone(), timeout.handle());
    let mut host = host(move || {
        let s = s.clone();
        CompositionLocalProvider(
            LOCAL_VIEW_CONFIGURATION.provides(long_press(t.get())),
            move || {
                let s = s.clone();
                Leaf(Modifier.size(50.dp()).pointer_input((), move |scope| {
                    let s = s.clone();
                    async move {
                        s.borrow_mut().push(scope.view_configuration().long_press_timeout_millis);
                        loop {
                            scope.await_pointer_event(PointerEventPass::Main).await;
                        }
                    }
                }));
            },
        );
    });
    let tap = |host: &mut UiHost| {
        host.send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
        host.send_pointer(PointerEvent::release(Offset::new(10.0, 10.0)));
        host.frame();
    };
    tap(&mut host);
    assert_eq!(&*seen.borrow(), &[1000]);
    timeout.set(2000);
    host.frame();
    tap(&mut host);
    assert_eq!(&*seen.borrow(), &[1000, 2000]);
}
