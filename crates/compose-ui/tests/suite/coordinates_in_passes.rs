//! Coordinates asked from inside the tree's passes, wherever application code runs there. Most of
//! these are about undefined behaviour, not their answers: they mean something under Miri
//! (docs/09-testing.md, "Miri"), where a query under a live reference to the tree is an error.
//! Natively they hold that nothing panics or overflows and that what's answered is right.

use crate::support;

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::*;
use support::*;

type Kept = Rc<Cell<Option<LayoutCoordinates>>>;

fn keep(k: &Kept) -> Modifier {
    let k = k.clone();
    Modifier.on_globally_positioned(move |c: LayoutCoordinates| k.set(Some(c)))
}

fn leaf(m: Modifier) {
    Layout(m, measure_policy(|s, _, _| s.layout(5, 5)), callback(|| {}));
}

fn moved(x: i32, y: i32) -> Modifier {
    Modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints);
        scope.place(&placeable, x, y);
        scope.layout(placeable.width, placeable.height)
    })
}

/// Asks where `kept` is, if it's known yet.
fn ask(kept: &Kept) -> Offset {
    kept.get().map_or(Offset::ZERO, |coordinates| coordinates.position_in_root())
}

/// A node told it was placed does `told` with its context.
struct Told(Rc<dyn Fn(&mut NodeContext<'_>)>);

impl ModifierNode for Told {
    fn as_layout_aware(&mut self) -> Option<&mut dyn LayoutAwareModifierNode> {
        Some(self)
    }
}

impl LayoutAwareModifierNode for Told {
    fn on_placed(&mut self, _: LayoutCoordinates, context: &mut NodeContext<'_>) {
        (self.0)(context);
    }
}

struct ToldElement(Rc<dyn Fn(&mut NodeContext<'_>)>);

impl ModifierNodeElement for ToldElement {
    type Node = Told;
    fn create(&self) -> Told {
        Told(self.0.clone())
    }
    fn update(&self, node: &mut Told) {
        node.0 = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// B1: nothing to do with coordinates: a guard outliving the context it opened the window for.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/focus/FocusTargetAttachDetachTest.kt, removedActiveFocusTarget_clearsFocus
#[test]
fn a_focused_node_removed_tells_its_listener_and_asks_nothing_of_the_window() {
    let shown = mutable_state_of(true);
    let requester = FocusRequester::new();
    let (s, r) = (shown.handle(), requester);
    let mut host = host(move || {
        let m = Modifier.focus_requester(r).on_focus_changed(|_| {});
        leaf(if s.get() { m.focus_target() } else { Modifier });
    });
    requester.request_focus();
    host.frame();
    shown.set(false);
    host.frame();
}

/// B2: a node a traversal found, in the caller's hands, with a query in between.
#[test]
fn a_query_while_holding_an_ancestor_found_is_sound() {
    static KEY: TraverseKey = TraverseKey::new("ancestor");
    struct Ancestor(i32);
    impl ModifierNode for Ancestor {
        fn as_traversable(&mut self) -> Option<&mut dyn TraversableNode> {
            Some(self)
        }
    }
    impl TraversableNode for Ancestor {
        fn traverse_key(&self) -> &'static TraverseKey {
            &KEY
        }
    }
    struct AncestorElement;
    impl ModifierNodeElement for AncestorElement {
        type Node = Ancestor;
        fn create(&self) -> Ancestor {
            Ancestor(1)
        }
        fn update(&self, _: &mut Ancestor) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    struct Finding(Kept, Rc<Cell<i32>>);
    impl ModifierNode for Finding {
        fn as_layout_aware(&mut self) -> Option<&mut dyn LayoutAwareModifierNode> {
            Some(self)
        }
    }
    impl LayoutAwareModifierNode for Finding {
        fn on_placed(&mut self, _: LayoutCoordinates, context: &mut NodeContext<'_>) {
            let Some(coordinates) = self.0.get() else { return };
            #[allow(unused_mut)]
            let mut found = context.find_nearest_ancestor_with_key(&KEY).expect("above it");
            let ancestor = found.downcast_mut::<Ancestor>().expect("of its type");
            // Part of the tree in hand: it's busy.
            assert_eq!(coordinates.position_in_root(), Offset::ZERO);
            ancestor.0 = 2;
            self.1.set(ancestor.0);
        }
    }
    struct FindingElement(Kept, Rc<Cell<i32>>);
    impl ModifierNodeElement for FindingElement {
        type Node = Finding;
        fn create(&self) -> Finding {
            Finding(self.0.clone(), self.1.clone())
        }
        fn update(&self, _: &mut Finding) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    let again = mutable_state_of(0);
    let kept: Kept = Rc::default();
    let seen = Rc::new(Cell::new(0));
    let (k, s, a) = (kept.clone(), seen.clone(), again.handle());
    let mut host = host(move || {
        let (k, s) = (k.clone(), s.clone());
        let moved = moved(a.get(), 0);
        leaf(
            moved
                .then(keep(&k))
                .then_custom(AncestorElement)
                .then_custom(FindingElement(k.clone(), s.clone())),
        );
    });
    again.set(1);
    host.frame();
    assert_eq!(seen.get(), 2);
}

/// B3: a graphics layer's block, run by the clip walk of an `on_globally_positioned`.
#[test]
fn a_layer_s_block_asking_under_a_position_callback_is_sound() {
    let kept: Kept = Rc::default();
    let seen = Rc::new(Cell::new(Offset::ZERO));
    let again = mutable_state_of(0);
    let (k, s, a) = (kept.clone(), seen.clone(), again.handle());
    let mut host = host(move || {
        let (k, s) = (k.clone(), s.clone());
        a.get();
        let (k2, s2) = (k.clone(), s.clone());
        leaf(moved(3, 4).graphics_layer(move |_| s2.set(ask(&k2))).then(keep(&k)).clip_to_bounds());
    });
    again.set(1);
    host.frame();
    assert_eq!(seen.get(), Offset::new(3.0, 4.0));
}

/// B3b: a modifier node's `hit_clip`, under the same walk.
#[test]
fn a_node_s_hit_clip_asking_under_a_position_callback_is_sound() {
    struct Clipping(Kept);
    impl ModifierNode for Clipping {
        fn hit_clip(&self) -> Option<Shape> {
            let _ = ask(&self.0);
            Some(Shape::Rectangle)
        }
    }
    struct ClippingElement(Kept);
    impl ModifierNodeElement for ClippingElement {
        type Node = Clipping;
        fn create(&self) -> Clipping {
            Clipping(self.0.clone())
        }
        fn update(&self, _: &mut Clipping) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    let kept: Kept = Rc::default();
    let again = mutable_state_of(0);
    let (k, a) = (kept.clone(), again.handle());
    let mut host = host(move || {
        a.get();
        leaf(moved(3, 4).then_custom(ClippingElement(k.clone())).then(keep(&k)));
    });
    again.set(1);
    host.frame();
    assert_eq!(ask(&kept), Offset::new(3.0, 4.0));
}

/// B4: a `focus_properties` block, under a request a node makes in `on_placed`.
#[test]
fn a_focus_properties_block_asking_under_a_request_is_sound() {
    let kept: Kept = Rc::default();
    let seen = Rc::new(Cell::new(0));
    let (k, s) = (kept.clone(), seen.clone());
    let told: Rc<dyn Fn(&mut NodeContext<'_>)> = Rc::new(|context| {
        context.request_focus(FocusDirection::Enter);
    });
    let offset = mutable_state_of(0);
    let o = offset.handle();
    let mut host = host(move || {
        let (k2, s2) = (k.clone(), s.clone());
        leaf(
            moved(o.get(), 0)
                .then(keep(&k))
                .then_custom(ToldElement(told.clone()))
                .focus_properties(move |_| {
                    let _ = ask(&k2);
                    s2.set(s2.get() + 1);
                })
                .focus_target(),
        );
    });
    for x in 1..3 {
        offset.set(x);
        host.frame();
    }
    assert!(seen.get() > 0, "its block asked");
}

/// What asks where it is when it's released and dropped.
struct AskingWhenGone(Kept);

impl ModifierNode for AskingWhenGone {
    fn on_release(&mut self, _: &mut ReleaseContext<'_>) {
        let _ = ask(&self.0);
    }
}

impl Drop for AskingWhenGone {
    fn drop(&mut self) {
        let _ = ask(&self.0);
    }
}

/// B5: a delegate let go in `on_placed`: its `on_release` and `Drop`.
#[test]
fn a_delegate_undelegated_asking_as_it_goes_is_sound() {
    struct Owner(Delegates, Kept, Option<Delegate<AskingWhenGone>>);
    impl ModifierNode for Owner {
        fn delegates(&mut self) -> Option<&mut Delegates> {
            Some(&mut self.0)
        }
        fn as_layout_aware(&mut self) -> Option<&mut dyn LayoutAwareModifierNode> {
            Some(self)
        }
    }
    impl LayoutAwareModifierNode for Owner {
        fn on_placed(&mut self, _: LayoutCoordinates, _: &mut NodeContext<'_>) {
            match self.2.take() {
                Some(delegate) => self.0.undelegate(delegate),
                None => self.2 = Some(self.0.delegate(AskingWhenGone(self.1.clone()))),
            }
        }
    }
    struct OwnerElement(Kept);
    impl ModifierNodeElement for OwnerElement {
        type Node = Owner;
        fn create(&self) -> Owner {
            Owner(Delegates::new(), self.0.clone(), None)
        }
        fn update(&self, _: &mut Owner) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    let kept: Kept = Rc::default();
    let offset = mutable_state_of(0);
    let (k, o) = (kept.clone(), offset.handle());
    let mut host = host(move || {
        leaf(moved(o.get(), 0).then(keep(&k)).then_custom(OwnerElement(k.clone())));
    });
    for x in 1..4 {
        offset.set(x);
        host.frame();
    }
}

/// B6: what an earlier block captured, dropped as a later one replaces it: asked with nothing of
/// the tree in hand, so it answers.
#[test]
fn an_earlier_block_s_capture_asking_as_it_drops_answers() {
    struct AskingCapture(Kept, Rc<Cell<Offset>>);
    impl Drop for AskingCapture {
        fn drop(&mut self) {
            self.1.set(ask(&self.0));
        }
    }
    let kept: Kept = Rc::default();
    let seen = Rc::new(Cell::new(Offset::new(-1.0, -1.0)));
    let again = mutable_state_of(0);
    let (k, s, a) = (kept.clone(), seen.clone(), again.handle());
    let mut host = host(move || {
        let (k2, s2) = (k.clone(), s.clone());
        Layout(
            moved(3, 4).then(keep(&k)),
            measure_policy(move |scope, _, _| {
                a.get();
                let capture = AskingCapture(k2.clone(), s2.clone());
                let _ = scope.layout_with(5, 5, move |_| {
                    let _ = &capture;
                });
                // The second replaces the first in this one measurement.
                scope.layout_with(5, 5, |_| {})
            }),
            callback(|| {}),
        );
    });
    again.set(1);
    host.frame();
    assert_eq!(seen.get(), Offset::new(3.0, 4.0));
}

thread_local! {
    static KEPT_FOR_A_LOCAL: Kept = Rc::default();
}

static LOCAL_ASKED: CompositionLocal<f32> =
    CompositionLocal::with_computed_default("asked", |_| {
        KEPT_FOR_A_LOCAL
            .with(|kept| kept.get().map_or(0.0, |coordinates| coordinates.bounds_in_root().right))
    });

/// B7: a computed local's closure asking while a scope reads the local.
#[test]
fn a_computed_local_asking_while_it_is_read_is_sound() {
    let again = mutable_state_of(0);
    let seen = Rc::new(Cell::new(0.0));
    let (s, a) = (seen.clone(), again.handle());
    let mut host = host(move || {
        let s = s.clone();
        let kept = KEPT_FOR_A_LOCAL.with(Clone::clone);
        Layout(
            keep(&kept).then(moved(3, 4)),
            measure_policy(move |scope, _, _| {
                a.get();
                s.set(scope.current_value_of(&LOCAL_ASKED));
                scope.layout(5, 5)
            }),
            callback(|| {}),
        );
    });
    again.set(1);
    host.frame();
    assert_eq!(seen.get(), 5.0);
}

/// H2: a layer's block asking where it is walks through itself in a query: not any more.
#[test]
fn a_layer_s_block_asking_where_it_is_does_not_recurse() {
    let kept: Kept = Rc::default();
    let seen = Rc::new(Cell::new(Offset::new(-1.0, -1.0)));
    let again = mutable_state_of(0);
    let (k, s, a) = (kept.clone(), seen.clone(), again.handle());
    let mut host = host(move || {
        let (k, s) = (k.clone(), s.clone());
        a.get();
        let k2 = k.clone();
        leaf(
            moved(3, 4)
                .graphics_layer(move |layer| {
                    layer.translation_x = 10.0;
                    s.set(ask(&k2));
                })
                .then(keep(&k)),
        );
    });
    again.set(1);
    host.frame();
    // Through its own translation, the last it gave: the matrix a query reads, as Compose's.
    assert_eq!(seen.get(), Offset::new(13.0, 4.0));
}

/// H3: hit testing asks a layer's clip: its last, without its block.
#[test]
fn a_press_doesn_t_run_a_layer_s_block() {
    let kept: Kept = Rc::default();
    let runs = Rc::new(Cell::new(0));
    let (k, r) = (kept.clone(), runs.clone());
    let mut host = host(move || {
        let (k, r) = (k.clone(), r.clone());
        let k2 = k.clone();
        leaf(
            Modifier
                .graphics_layer(move |layer| {
                    r.set(r.get() + 1);
                    layer.clip = true;
                    let _ = ask(&k2);
                })
                .then(keep(&k))
                .clickable(|| {}),
        );
    });
    let before = runs.get();
    host.send_pointer(PointerEvent::press(Offset::new(1.0, 1.0)));
    assert_eq!(runs.get(), before, "drawing alone runs it");
}

/// A node removed from its chain asking its parent's data be looked at again, in its detach.
#[test]
fn invalidating_parent_data_as_a_node_detaches_is_sound() {
    struct Detaching(Kept);
    impl ModifierNode for Detaching {
        fn on_detach(&mut self, context: &mut NodeContext<'_>) {
            let _ = ask(&self.0);
            context.invalidate_parent_data();
        }
    }
    struct DetachingElement(Kept);
    impl ModifierNodeElement for DetachingElement {
        type Node = Detaching;
        fn create(&self) -> Detaching {
            Detaching(self.0.clone())
        }
        fn update(&self, _: &mut Detaching) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    let on = mutable_state_of(true);
    let kept: Kept = Rc::default();
    let (k, o) = (kept.clone(), on.handle());
    let mut host = host(move || {
        let modifier =
            if o.get() { Modifier.then_custom(DetachingElement(k.clone())) } else { Modifier };
        leaf(keep(&k).then(modifier));
    });
    on.set(false);
    host.frame();
}

/// Asked in each place a tree's pass runs application code that isn't a callback: never a panic.
#[test]
fn an_update_a_parent_data_and_a_semantics_block_ask_without_a_panic() {
    struct Updating(Kept);
    impl ModifierNode for Updating {}
    struct UpdatingElement(Kept, i32);
    impl ModifierNodeElement for UpdatingElement {
        type Node = Updating;
        fn create(&self) -> Updating {
            Updating(self.0.clone())
        }
        fn update(&self, node: &mut Updating) {
            let _ = ask(&node.0);
        }
        fn same(&self, other: &Self) -> bool {
            self.1 == other.1
        }
    }
    let kept: Kept = Rc::default();
    let again = mutable_state_of(0);
    let (k, a) = (kept.clone(), again.handle());
    let mut host = host(move || {
        let k2 = k.clone();
        leaf(keep(&k).then_custom(UpdatingElement(k.clone(), a.get())).semantics(move |_| {
            let _ = ask(&k2);
        }));
    });
    again.set(1);
    host.frame();
    let _ = host.semantics_tree(true);
}

// ---- A panic, and what's in the chain ---------------------------------------------------------------

/// F1: a draw node's panic leaves it in its chain: it draws again, and its end comes.
#[test]
fn a_draw_node_that_panicked_is_still_there() {
    struct Drawing(Rc<std::cell::RefCell<Vec<&'static str>>>, MutableState<bool>);
    impl ModifierNode for Drawing {
        fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
            Some(self)
        }
        fn on_detach(&mut self, _: &mut NodeContext<'_>) {
            self.0.borrow_mut().push("on_detach");
        }
        fn on_release(&mut self, _: &mut ReleaseContext<'_>) {
            self.0.borrow_mut().push("on_release");
        }
    }
    impl DrawModifierNode for Drawing {
        fn draw(&mut self, _: &mut DrawScope<'_>) {
            self.0.borrow_mut().push("draw");
            assert!(!self.1.get(), "asked to");
        }
    }
    impl Drop for Drawing {
        fn drop(&mut self) {
            self.0.borrow_mut().push("drop");
        }
    }
    struct DrawingElement(Rc<std::cell::RefCell<Vec<&'static str>>>, MutableState<bool>);
    impl ModifierNodeElement for DrawingElement {
        type Node = Drawing;
        fn create(&self) -> Drawing {
            Drawing(self.0.clone(), self.1)
        }
        fn update(&self, _: &mut Drawing) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    let log: Rc<std::cell::RefCell<Vec<&'static str>>> = Rc::default();
    let (panics, shown, redraw) =
        (mutable_state_of(false), mutable_state_of(true), mutable_state_of(0));
    let (l, p, s, r) = (log.clone(), panics.handle(), shown.handle(), redraw.handle());
    let mut host = host(move || {
        let modifier = Modifier.draw_behind(move |_| {
            r.get();
        });
        if s.get() {
            leaf(modifier.then_custom(DrawingElement(l.clone(), p)));
        }
    });
    log.borrow_mut().clear();
    panics.set(true);
    redraw.set(1);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        host.frame();
    }));
    assert!(result.is_err());
    assert_eq!(*log.borrow(), ["draw"]);
    log.borrow_mut().clear();
    panics.set(false);
    redraw.set(2);
    host.frame();
    assert_eq!(*log.borrow(), ["draw"], "it draws again");
    log.borrow_mut().clear();
    shown.set(false);
    host.frame();
    assert_eq!(*log.borrow(), ["on_detach", "on_release", "drop"]);
}

// ---- A layer's last run ------------------------------------------------------------------------------

struct Holding(Kept);
impl ModifierNode for Holding {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.0.set(Some(context.require_layout_coordinates()));
    }
}
struct HoldingElement(Kept);
impl ModifierNodeElement for HoldingElement {
    type Node = Holding;
    fn create(&self) -> Holding {
        Holding(self.0.clone())
    }
    fn update(&self, _: &mut Holding) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

/// F2: a block's first run in a frame that changed a layer around it sees the layer's new value,
/// as Compose's runs as the write applies.
// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/NodeCoordinator.kt, onCommitAffectingLayerParams
#[test]
fn a_first_block_sees_a_layer_changed_in_its_frame() {
    let (translation, show) = (mutable_state_of(0.0f32), mutable_state_of(false));
    let kept: Kept = Rc::default();
    let seen = Rc::new(Cell::new(Offset::new(-1.0, -1.0)));
    let (t, sh, k, se) = (translation.handle(), show.handle(), kept.clone(), seen.clone());
    let mut host = host(move || {
        let (k, se) = (k.clone(), se.clone());
        let (k2, se2) = (k.clone(), se.clone());
        Layout(
            Modifier
                .graphics_layer(move |layer| layer.translation_x = t.get())
                .then_custom(HoldingElement(k)),
            measure_policy(move |scope, _, _| {
                if sh.get() {
                    let (k, se) = (k2.clone(), se2.clone());
                    scope.layout_with(5, 5, move |_| se.set(ask(&k)))
                } else {
                    scope.layout(5, 5)
                }
            }),
            callback(|| {}),
        );
    });
    translation.set(9.0);
    show.set(true);
    host.frame();
    host.frame();
    host.frame();
    assert_eq!(seen.get(), Offset::new(9.0, 0.0));
}

/// F3: a clipped layer's block no frame drew: a press asks it, once.
#[test]
fn a_layer_never_drawn_clips_a_press() {
    struct RightHalf;
    impl CustomShape for RightHalf {
        fn create_outline(&self, size: Size, _: LayoutDirection, _: &Density) -> Outline {
            Outline::Rectangle(Rect::new(size.width / 2.0, 0.0, size.width, size.height))
        }
        fn same(&self, _: &dyn CustomShape) -> bool {
            true
        }
    }
    let clicks = Rc::new(Cell::new(0));
    let c = clicks.clone();
    let mut host = host(move || {
        let c = c.clone();
        Stack!(modifier = Modifier.draw_with_content(|_| {}), {
            let c = c.clone();
            Leaf(
                Modifier
                    .size(10.dp())
                    .graphics_layer(|layer| {
                        layer.clip = true;
                        layer.shape = Shape::custom(RightHalf);
                    })
                    .clickable(move || c.set(c.get() + 1)),
            );
        });
    });
    click(&mut host, 2.0, 5.0);
    assert_eq!(clicks.get(), 0, "outside its shape");
    click(&mut host, 8.0, 5.0);
    assert_eq!(clicks.get(), 1, "inside it");
}

/// A press tests against the shape a block last gave, running neither it nor the shape's code.
// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/OwnedLayer.kt, isInLayer
#[test]
fn a_press_is_clipped_by_the_last_shape_a_block_gave() {
    struct Half(bool, Rc<Cell<u32>>);
    impl CustomShape for Half {
        fn create_outline(&self, size: Size, _: LayoutDirection, _: &Density) -> Outline {
            self.1.set(self.1.get() + 1);
            let middle = size.width / 2.0;
            Outline::Rectangle(if self.0 {
                Rect::new(0.0, 0.0, middle, size.height)
            } else {
                Rect::new(middle, 0.0, size.width, size.height)
            })
        }
        fn same(&self, _: &dyn CustomShape) -> bool {
            false
        }
    }
    let left = mutable_state_of(true);
    let (clicks, outlines) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (c, o, l) = (clicks.clone(), outlines.clone(), left.handle());
    let mut host = host(move || {
        let (c, o) = (c.clone(), o.clone());
        Leaf(
            Modifier
                .size(10.dp())
                .graphics_layer(move |layer| {
                    layer.clip = true;
                    layer.shape = Shape::custom(Half(l.get(), o.clone()));
                })
                .clickable(move || c.set(c.get() + 1)),
        );
    });
    let press = |host: &mut compose_ui::platform::UiHost, x: f32| {
        host.send_pointer(PointerEvent::press(Offset::new(x, 5.0)));
        host.send_pointer(PointerEvent::release(Offset::new(x, 5.0)));
    };
    let before = outlines.get();
    press(&mut host, 2.0);
    assert_eq!(clicks.get(), 1, "in the left half");
    press(&mut host, 8.0);
    assert_eq!(clicks.get(), 1, "not in the right");
    assert_eq!(outlines.get(), before, "a press runs no shape");
    host.frame();
    left.set(false);
    host.frame();
    press(&mut host, 2.0);
    assert_eq!(clicks.get(), 1, "not in the left now");
    press(&mut host, 8.0);
    assert_eq!(clicks.get(), 2, "in the right");
}

/// F5: a block's clip flipping tells who listens: its bounds changed, not its transform.
// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/NodeCoordinator.kt, onCommitAffectingLayerParams
#[test]
fn a_layer_s_clip_flipping_tells_a_position_listener() {
    let clip = mutable_state_of(false);
    let bounds = Rc::new(Cell::new(Rect::new(0.0, 0.0, 0.0, 0.0)));
    let (c, b) = (clip.handle(), bounds.clone());
    let mut host = host(move || {
        let b = b.clone();
        Layout(
            sized_px(10).graphics_layer(move |layer| layer.clip = c.get()),
            measure_policy(|scope, measurables, constraints| {
                let placeable = scope.measure(measurables[0], constraints.loosen());
                scope.place(&placeable, 8, 0);
                scope.layout(10, 10)
            }),
            callback(move || {
                let b = b.clone();
                leaf(Modifier.on_globally_positioned(move |coordinates: LayoutCoordinates| {
                    b.set(coordinates.bounds_in_window());
                }));
            }),
        );
    });
    assert_eq!(bounds.get().right, 13.0);
    clip.set(true);
    host.frame();
    assert_eq!(bounds.get().right, 10.0);
    clip.set(false);
    host.frame();
    assert_eq!(bounds.get().right, 13.0);
}

fn sized_px(size: i32) -> Modifier {
    Modifier.size((size as f32).dp())
}

/// F7: an inner layer's block runs once for an update, a listener anywhere or not: a recomposition
/// giving it a new one, and a write to what it reads.
#[test]
fn an_inner_layer_runs_once_per_update_with_a_listener() {
    let (composed, value) = (mutable_state_of(0.0f32), mutable_state_of(0.0f32));
    let runs = Rc::new(Cell::new(0));
    let (c, v, r) = (composed.handle(), value.handle(), runs.clone());
    let mut host = host(move || {
        let r = r.clone();
        let translation = c.get();
        leaf(Modifier.on_globally_positioned(|_: LayoutCoordinates| {}));
        leaf(
            // A drawing before it makes the layer an inner one.
            Modifier.draw_behind(|_| {}).graphics_layer(move |layer| {
                r.set(r.get() + 1);
                layer.translation_x = translation + v.get();
            }),
        );
    });
    runs.set(0);
    composed.set(3.0);
    host.frame();
    assert_eq!(runs.get(), 1, "a new block");
    runs.set(0);
    value.set(4.0);
    host.frame();
    assert_eq!(runs.get(), 1, "a write");
    // It still hears the next, its reads kept from the run ahead.
    runs.set(0);
    value.set(5.0);
    host.frame();
    assert_eq!(runs.get(), 1, "another");
    runs.set(0);
    host.frame();
    assert_eq!(runs.get(), 0, "at rest");
}

// ---- Busy, and other threads ---------------------------------------------------------------------

#[test]
fn busy_says_not_attached() {
    let kept: Kept = Rc::default();
    let k = kept.clone();
    let host = host(move || leaf(keep(&k)));
    let coordinates = kept.get().expect("positioned");
    assert!(coordinates.is_attached());
    assert!(!coordinates.is_busy());
    host.with_tree(|_| {
        assert!(!coordinates.is_attached());
        assert!(coordinates.is_busy());
    });
    assert!(coordinates.is_attached());
}

/// F6: a semantics node and a focus properties block have a window: what they ask they're told.
#[test]
fn a_semantics_node_and_a_focus_properties_block_ask_and_are_told() {
    struct Asking(Kept, Rc<Cell<Offset>>, MutableState<i32>);
    impl ModifierNode for Asking {
        fn as_semantics(&mut self) -> Option<&mut dyn SemanticsModifierNode> {
            Some(self)
        }
    }
    impl SemanticsModifierNode for Asking {
        fn apply_semantics(&self, _: &mut SemanticsConfiguration) {
            self.2.get();
            self.1.set(ask(&self.0));
        }
    }
    struct AskingElement(Kept, Rc<Cell<Offset>>, MutableState<i32>);
    impl ModifierNodeElement for AskingElement {
        type Node = Asking;
        fn create(&self) -> Asking {
            Asking(self.0.clone(), self.1.clone(), self.2)
        }
        fn update(&self, _: &mut Asking) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    let kept: Kept = Rc::default();
    let (semantics, properties) =
        (Rc::new(Cell::new(Offset::ZERO)), Rc::new(Cell::new(Offset::ZERO)));
    let (k, s, p) = (kept.clone(), semantics.clone(), properties.clone());
    let requester = FocusRequester::new();
    let again = mutable_state_of(0);
    let a = again.handle();
    let mut host = host(move || {
        let (k2, p) = (k.clone(), p.clone());
        leaf(
            moved(3, 4)
                .then(keep(&k))
                .then_custom(AskingElement(k.clone(), s.clone(), a))
                .focus_requester(requester)
                .focus_properties(move |_| p.set(ask(&k2)))
                .focus_target(),
        );
    });
    let _ = host.semantics_tree(true);
    // Once what it keeps is known.
    again.set(1);
    host.frame();
    let _ = host.semantics_tree(true);
    requester.request_focus();
    host.frame();
    assert_eq!(semantics.get(), Offset::new(3.0, 4.0));
    assert_eq!(properties.get(), Offset::new(3.0, 4.0));
}

thread_local! {
    static ASKS_FIRST: std::cell::RefCell<Option<AskingAtExit>> =
        const { std::cell::RefCell::new(None) };
    static ASKS_LAST: std::cell::RefCell<Option<AskingAtExit>> =
        const { std::cell::RefCell::new(None) };
}

struct AskingAtExit(LayoutCoordinates);

impl Drop for AskingAtExit {
    fn drop(&mut self) {
        let _ = self.0.position_in_root();
        let _ = self.0.is_attached();
    }
}

/// F8: a query in a `Drop` as the thread exits, its registry gone or going: no abort. One of the
/// two goes after the registry, whichever order a platform destroys them in.
#[test]
fn a_query_at_thread_exit_does_not_abort() {
    let thread = std::thread::spawn(|| {
        ASKS_FIRST.with(|_| {});
        let kept: Kept = Rc::default();
        let k = kept.clone();
        let host = host(move || leaf(keep(&k)));
        let coordinates = kept.get().expect("positioned");
        ASKS_FIRST.with(|slot| *slot.borrow_mut() = Some(AskingAtExit(coordinates)));
        ASKS_LAST.with(|slot| *slot.borrow_mut() = Some(AskingAtExit(coordinates)));
        std::mem::forget(host);
    });
    assert!(thread.join().is_ok());
}

/// F2 again: a measure policy reading its own state in the frame a layer changed sees it too.
#[test]
fn a_measure_policy_sees_a_layer_changed_in_its_frame() {
    let (translation, again) = (mutable_state_of(0.0f32), mutable_state_of(0));
    let kept: Kept = Rc::default();
    let seen = Rc::new(Cell::new(Offset::new(-1.0, -1.0)));
    let (t, a, k, se) = (translation.handle(), again.handle(), kept.clone(), seen.clone());
    let mut host = host(move || {
        let (k2, se) = (k.clone(), se.clone());
        Layout(
            Modifier
                .graphics_layer(move |layer| layer.translation_x = t.get())
                .then_custom(HoldingElement(k.clone())),
            measure_policy(move |scope, _, _| {
                a.get();
                se.set(ask(&k2));
                scope.layout(5, 5)
            }),
            callback(|| {}),
        );
    });
    translation.set(9.0);
    again.set(1);
    host.frame();
    assert_eq!(seen.get(), Offset::new(9.0, 0.0));
}

/// A layer a draw ran isn't run again for coordinates first given later.
#[test]
fn a_layer_drawn_isn_t_run_again_for_a_later_listener() {
    let later = mutable_state_of(false);
    let runs = Rc::new(Cell::new(0));
    let (l, r) = (later.handle(), runs.clone());
    let mut host = host(move || {
        let r = r.clone();
        leaf(Modifier.graphics_layer(move |_| r.set(r.get() + 1)));
        // Its own scope: the layer isn't composed again.
        Stack!({
            if l.get() {
                leaf(Modifier.on_globally_positioned(|_: LayoutCoordinates| {}));
            }
        });
    });
    let before = runs.get();
    later.set(true);
    host.frame();
    assert_eq!(runs.get(), before);
}

/// F9: a handle's tree is its own, whatever thread asks.
#[test]
fn a_handle_read_on_another_thread_names_no_tree_there() {
    let kept: Kept = Rc::default();
    let k = kept.clone();
    let _host = host(move || leaf(keep(&k)));
    let bits = kept.get().expect("positioned");
    let thread = std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let other: Kept = Rc::default();
                let o = other.clone();
                let _host = host(move || leaf(keep(&o)));
                bits.is_attached()
            })
            .join()
    });
    assert_eq!(thread.ok(), Some(false));
}

/// A measure reading where a child is, which only a `layout_with` block moved: what it last placed,
/// and asking measures nothing again by itself.
#[test]
fn a_measure_reads_where_a_block_put_a_child() {
    let offset = mutable_state_of(0);
    let again = mutable_state_of(0);
    let kept: Rc<Cell<Option<LayoutCoordinates>>> = Rc::default();
    let (seen, measures) = (Rc::new(Cell::new(Offset::ZERO)), Rc::new(Cell::new(0)));
    let (o, a, k, s, m) =
        (offset.handle(), again.handle(), kept.clone(), seen.clone(), measures.clone());
    let mut host = support::host(move || {
        let (k, s, m) = (k.clone(), s.clone(), m.clone());
        let k2 = k.clone();
        let policy = measure_policy(move |scope, measurables, constraints| {
            a.get();
            m.set(m.get() + 1);
            if let Some(coordinates) = k2.get() {
                s.set(coordinates.position_in_root());
            }
            let placeable = scope.measure(measurables[0], constraints);
            scope.place(&placeable, 0, 0);
            scope.layout(placeable.width, placeable.height)
        });
        Layout(
            Modifier,
            policy,
            callback(move || {
                let k = k.clone();
                let keeping = Modifier
                    .offset_with(move |_| IntOffset::new(o.get(), 0))
                    .on_placed(move |coordinates: LayoutCoordinates| k.set(Some(coordinates)));
                Layout(keeping, empty_measure_policy(), callback(|| {}));
            }),
        );
    });
    offset.set(30);
    host.frame();
    assert_eq!(measures.get(), 1, "a move measures nothing");
    again.set(1);
    host.frame();
    assert_eq!(measures.get(), 2);
    assert_eq!(seen.get(), Offset::new(30.0, 0.0));
}
