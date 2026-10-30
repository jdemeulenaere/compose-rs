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

/// B6: what an earlier measurement's block captured, dropped as the next one replaces it.
#[test]
fn an_earlier_block_s_capture_asking_as_it_drops_is_sound() {
    struct AskingCapture(Kept);
    impl Drop for AskingCapture {
        fn drop(&mut self) {
            let _ = ask(&self.0);
        }
    }
    let kept: Kept = Rc::default();
    let size = mutable_state_of(5);
    let (k, s) = (kept.clone(), size.handle());
    let mut host = host(move || {
        let k2 = k.clone();
        Layout(
            keep(&k),
            measure_policy(move |scope, _, _| {
                let capture = AskingCapture(k2.clone());
                scope.layout_with(s.get(), 5, move |_| {
                    let _ = &capture;
                })
            }),
            callback(|| {}),
        );
    });
    size.set(6);
    host.frame();
    size.set(7);
    host.frame();
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
