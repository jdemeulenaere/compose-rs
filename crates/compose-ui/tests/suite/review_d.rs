//! Phase D review: failing tests for the UI core (docs/06-ui.md). Each test states the input,
//! what Compose (or the docs) expects, and what happens instead.

#![allow(non_snake_case)]

use crate::support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::platform::*;
use compose_ui::*;
use support::*;

fn first_child(host: &UiHost) -> NodeId {
    host.with_tree(|tree| tree.children_of(tree.root_id()).next().unwrap())
}

fn focused_count(host: &UiHost) -> usize {
    host.semantics_tree(true)
        .flatten()
        .iter()
        .filter(|node| node.config.get(&compose_ui::SemanticsProperties::FOCUSED) == Some(&true))
        .count()
}

// ---- Incremental measurement -------------------------------------------------------------------

/// A child measured outside its constraints is centered in the size its parent sees. When its
/// measured size changes but the coerced size doesn't, `measure_node` re-measures it in place
/// and stops there (the parent's policy doesn't run, so nobody calls `place` again), leaving
/// the centering offset of the old measured size.
#[test]
fn a_required_size_change_in_a_fixed_parent_recenters_the_child() {
    let width = mutable_state_of(100.0f32);
    let handle = width.handle();
    let mut changed = host(move || {
        Stack!(modifier = Modifier.size(50.dp()), {
            Leaf(Modifier.required_size((handle.get().dp(), 10.dp())));
        });
    });
    width.set(120.0);
    changed.frame();
    let fresh = host(|| {
        Stack!(modifier = Modifier.size(50.dp()), {
            Leaf(Modifier.required_size((120.dp(), 10.dp())));
        });
    });
    // Fresh: the leaf at x = (50 - 120) / 2 = -35. Incremental: still at -25.
    assert_eq!(changed.dump_tree(), fresh.dump_tree());
}

/// A policy measuring a child twice (here to learn its natural width, then with fixed
/// constraints). Compose rejects a second `measure` of the same measurable; compose-ui accepts
/// it, keeps only the last constraints, and the in-place remeasure with those hides the change
/// the first measurement would have seen: the parent keeps its old size.
struct MeasureTwicePolicy;

impl MeasurePolicy for MeasureTwicePolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        _constraints: Constraints,
    ) -> MeasureResult {
        let natural = scope.measure(measurables[0], Constraints::unbounded());
        let fixed = scope.measure(measurables[0], Constraints::fixed(10, 10));
        scope.place(&fixed, 0, 0);
        scope.layout(natural.width, natural.height)
    }
}

#[test]
#[should_panic(expected = "measured the same child twice")]
fn measuring_a_child_twice_is_rejected_or_stays_correct() {
    thread_local! {
        static POLICY: Rc<dyn MeasurePolicy> = Rc::new(MeasureTwicePolicy);
    }
    let width = mutable_state_of(20.0f32);
    let handle = width.handle();
    let mut changed = host(move || {
        Layout(
            Modifier,
            POLICY.with(Rc::clone),
            callback(move || Leaf(Modifier.size((handle.get().dp(), 5.dp())))),
        );
    });
    width.set(30.0);
    changed.frame();
    let fresh = host(|| {
        Layout(
            Modifier,
            POLICY.with(Rc::clone),
            callback(|| Leaf(Modifier.size((30.dp(), 5.dp())))),
        );
    });
    // Fresh: the parent is 30 wide. Incremental: still 20.
    assert_eq!(changed.dump_tree(), fresh.dump_tree());
}

// ---- Hit testing -------------------------------------------------------------------------------

/// `offset` moves its content, and in Compose the clickable after it is hit where it's drawn
/// (the offset's coordinator has no pointer input, so the hit test goes on into the wrapped
/// coordinator). compose-ui first requires the point to be inside the node's outermost layer
/// (`hit_node`'s `contains(value.layers[0].measured, local)`), which `offset` doesn't move, so
/// the drawn clickable is never hit; `perform_click` on its semantics center misses too.
#[test]
fn a_clickable_after_offset_is_hit_where_it_is_drawn() {
    let mut host = host(|| {
        Leaf(Modifier.offset(100.dp(), 0.dp()).size(20.dp()).clickable(|| log("click")));
    });
    // The display list puts the content at x = 100..120.
    click(&mut host, 110.0, 10.0);
    assert_eq!(take_log(), ["click"]);
}

/// A 20 x 20 node placing its children at x = 30, outside itself.
struct PlaceOutsidePolicy;

impl MeasurePolicy for PlaceOutsidePolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        for &measurable in measurables {
            let placeable = scope.measure(measurable, constraints.loosen());
            scope.place(&placeable, 30, 0);
        }
        scope.layout(20, 20)
    }
}

/// The same for a child placed outside its parent: Compose hit-tests children whatever their
/// parent's bounds (only a clip stops it), so the child is clicked where it's drawn. compose-ui
/// rejects the point at the parent's bounds and never visits the child (docs/06 states this
/// rule, but no decision records the divergence from Compose).
#[test]
fn a_child_outside_its_parent_is_hit_where_it_is_drawn() {
    let mut host = host(|| {
        Layout(
            Modifier,
            Rc::new(PlaceOutsidePolicy),
            callback(|| Leaf(Modifier.size(10.dp()).clickable(|| log("click")))),
        );
    });
    click(&mut host, 35.0, 5.0);
    assert_eq!(take_log(), ["click"]);
}

/// `clip(shape)` is a graphics layer in Compose, and a layer clips hit testing to its outline:
/// a click in the corner of a circle-clipped clickable doesn't click. compose-ui ignores clips
/// when hit testing.
#[test]
fn a_click_outside_a_rounded_clip_does_not_click() {
    let mut host = host(|| {
        Leaf(
            Modifier
                .size(100.dp())
                .clip(Shape::Rounded([CornerSize::Percent(50.0); 4]))
                .clickable(|| log("click")),
        );
    });
    click(&mut host, 2.0, 2.0);
    assert!(take_log().is_empty(), "the corner is outside the circle");
}

// ---- Focus -------------------------------------------------------------------------------------

/// Compose clears focus when the focused target can't take it any more. Through the fast path
/// of `set_modifier` (same kinds), `focusable(true)` becoming `focusable(false)` keeps the
/// focus, and the semantics still say focused.
#[test]
fn a_focused_node_that_becomes_unfocusable_loses_focus() {
    let enabled = mutable_state_of(true);
    let handle = enabled.handle();
    let mut host = host(move || Leaf(Modifier.size(10.dp()).focusable_with(handle.get())));
    host.send_key(KeyEvent::down(Key::Tab));
    assert!(host.with_tree(|tree| tree.focused()).is_some());
    enabled.set(false);
    host.frame();
    assert_eq!(host.with_tree(|tree| tree.focused()), None);
    assert_eq!(focused_count(&host), 0);
}

/// The same with a clickable disabled while focused.
#[test]
fn a_focused_clickable_that_is_disabled_loses_focus() {
    let enabled = mutable_state_of(true);
    let handle = enabled.handle();
    let mut host = host(move || {
        Leaf(Modifier.size(10.dp()).clickable_with(
            ClickableOptions { enabled: handle.get(), ..ClickableOptions::DEFAULT },
            || {},
        ));
    });
    host.send_key(KeyEvent::down(Key::Tab));
    enabled.set(false);
    host.frame();
    assert_eq!(focused_count(&host), 0, "{}", host.semantics_tree(true).dump());
}

/// A focus target inserted before the focused one in the chain: `focus_target_index` now
/// names the new one, so the old target (the clickable) is never told it lost focus. After Tab
/// moves focus elsewhere, two nodes say they're focused.
#[test]
fn a_focus_target_inserted_before_the_focused_one_doesnt_leave_it_focused() {
    let extra = mutable_state_of(false);
    let handle = extra.handle();
    let mut host = host(move || {
        Column!({
            let modifier = if handle.get() { Modifier.focusable() } else { Modifier };
            Leaf(modifier.size(10.dp()).clickable(|| {}));
            Leaf(Modifier.size(10.dp()).clickable(|| {}));
        });
    });
    host.send_key(KeyEvent::down(Key::Tab));
    extra.set(true);
    host.frame();
    host.send_key(KeyEvent::down(Key::Tab));
    host.frame();
    assert_eq!(focused_count(&host), 1, "{}", host.semantics_tree(true).dump());
}

/// Removing the focused clickable from a chain that keeps an `on_focus_changed` before it:
/// `set_modifier` sets `focused = None` without notifying, so the listener never hears
/// `false` (Compose reports the state change).
#[test]
fn losing_the_focus_target_from_the_chain_notifies_on_focus_changed() {
    let clickable = mutable_state_of(true);
    let handle = clickable.handle();
    let mut host = host(move || {
        let tail = if handle.get() { Modifier.clickable(|| {}) } else { Modifier };
        Leaf(
            Modifier
                .size(10.dp())
                .on_focus_changed(|focused| log(format!("focused {focused}")))
                .then(tail),
        );
    });
    assert_eq!(take_log(), ["focused false"], "the initial state");
    host.send_key(KeyEvent::down(Key::Tab));
    assert_eq!(take_log(), ["focused true"]);
    clickable.set(false);
    host.frame();
    assert_eq!(host.with_tree(|tree| tree.focused()), None);
    assert_eq!(take_log(), ["focused false"]);
}

/// Compose's `onFocusChanged` observes the focus targets below it, its node's children
/// included: `Box(Modifier.onFocusChanged { … }) { Button(…) }` hears the button's focus
/// (`hasFocus`). compose-ui only looks at targets later in the same chain.
#[test]
fn on_focus_changed_observes_a_focused_child() {
    let mut host = host(|| {
        Stack!(modifier = Modifier.on_focus_changed(|focused| log(format!("parent {focused}"))), {
            Leaf(Modifier.size(10.dp()).clickable(|| {}));
        });
    });
    take_log();
    host.send_key(KeyEvent::down(Key::Tab));
    assert!(host.with_tree(|tree| tree.focused()).is_some());
    assert_eq!(take_log(), ["parent true"]);
}

/// Compose calls `onFocusChanged` once when the node is attached, with the initial (inactive)
/// state; compose-ui only calls it on changes.
#[test]
fn on_focus_changed_reports_the_initial_state() {
    let _host = host(|| {
        Leaf(
            Modifier
                .size(10.dp())
                .on_focus_changed(|focused| log(format!("focused {focused}")))
                .focusable(),
        );
    });
    assert_eq!(take_log(), ["focused false"]);
}

/// In Compose, key events reach the key input nodes after the focus target in its chain
/// (`lastLocalKeyInputNode`), as well as those before it and in ancestors:
/// `Modifier.focusable().onKeyEvent { … }` gets the keys. compose-ui's `key_path` only takes
/// chain indices up to the focus target.
#[test]
fn on_key_event_after_focusable_receives_keys() {
    let mut host = host(|| {
        Leaf(Modifier.size(10.dp()).focusable().on_key_event(|event: KeyEvent| {
            log(format!("{:?}", event.key));
            true
        }));
    });
    let leaf = first_child(&host);
    host.request_focus(leaf);
    assert!(host.send_key(KeyEvent::down(Key::Character('a'))));
    assert_eq!(take_log(), ["Character('a')"]);
}

/// A focus target that grows when focused, asking for a measurement from `on_focus_changed`.
struct GrowOnFocus;

struct GrowOnFocusNode {
    focused: bool,
}

impl ModifierNodeElement for GrowOnFocus {
    type Node = GrowOnFocusNode;
    fn create(&self) -> GrowOnFocusNode {
        GrowOnFocusNode { focused: false }
    }
    fn update(&self, _: &mut Self::Node) {}
    fn same(&self, _other: &Self) -> bool {
        true
    }
}

impl ModifierNode for GrowOnFocusNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for GrowOnFocusNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        _: Constraints,
    ) -> MeasureResult {
        let side = if self.focused { 20 } else { 10 };
        let placeable = scope.measure(content, Constraints::fixed(side, side));
        scope.place(&placeable, 0, 0);
        scope.layout(side, side)
    }
}

impl FocusTargetModifierNode for GrowOnFocusNode {
    fn on_focus_changed(&mut self, focused: bool, context: &mut EventContext<'_>) {
        self.focused = focused;
        context.invalidate_measure();
    }
}

/// `EventContext::invalidate_measure` (and `request_focus`, `clear_focus`) from a focus
/// target's `on_focus_changed` are dropped: `notify_focus` applies only `requests.draw`.
#[test]
fn a_focus_target_can_ask_for_a_measurement_when_focused() {
    let mut host = host(|| Leaf(Modifier.then_custom(GrowOnFocus)));
    host.send_key(KeyEvent::down(Key::Tab));
    assert!(host.with_tree(|tree| tree.focused()).is_some());
    host.frame();
    let node = first_child(&host);
    assert_eq!(host.with_tree(|tree| tree.bounds_in_root(node)).width(), 20.0);
}

// ---- The frame waker ---------------------------------------------------------------------------

/// `set_frame_waker` is how a platform host learns it needs a frame. A focus change makes
/// `needs_frame()` true (the focus indication must be drawn) without calling the waker: only
/// state writes reach it. The same holds for `resize`, `set_density` and `set_content`.
#[test]
fn a_focus_change_wakes_the_platform() {
    let mut host = host(|| Leaf(Modifier.size(10.dp()).clickable(|| {})));
    let wakes = Rc::new(Cell::new(0));
    let counter = wakes.clone();
    host.set_frame_waker(move || counter.set(counter.get() + 1));
    host.send_key(KeyEvent::down(Key::Tab));
    assert!(host.needs_frame());
    assert!(wakes.get() > 0, "needs_frame() is true, but the waker wasn't called");
}

#[test]
fn a_resize_wakes_the_platform() {
    let mut host = host(|| Leaf(Modifier.fill_max_size()));
    let wakes = Rc::new(Cell::new(0));
    let counter = wakes.clone();
    host.set_frame_waker(move || counter.set(counter.get() + 1));
    host.resize(IntSize::new(300, 300));
    assert!(host.needs_frame());
    assert!(wakes.get() > 0, "needs_frame() is true, but the waker wasn't called");
}

// ---- Modifiers ---------------------------------------------------------------------------------

/// `Modifier.semantics` wraps its closure in a new `Rc` on every call and compares by pointer,
/// so a semantics modifier is never `same` as last pass's: a composable receiving one never
/// skips, and the node is updated and re-measured on every pass. (Inside a composable the
/// closure can't even be written: see review_d_compile_semantics_closure.rs.) Compose's
/// compiler remembers the lambda, so the receiver skips.
#[test]
fn a_semantics_modifier_lets_its_receiver_skip() {
    thread_local! {
        static RUNS: Cell<u32> = const { Cell::new(0) };
    }
    fn labelled() -> Modifier {
        Modifier.size(10.dp()).semantics(|config| config.set_text("label"))
    }
    #[composable]
    fn Receiver(modifier: Modifier) {
        RUNS.with(|runs| runs.set(runs.get() + 1));
        Leaf(modifier);
    }
    #[composable]
    fn Screen(tick: MutableState<u32>) {
        tick.get();
        Receiver(labelled());
    }
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    let mut host = host(move || Screen(handle));
    for round in 1..4 {
        tick.set(round);
        host.frame();
    }
    assert_eq!(RUNS.with(Cell::get), 1);
}

/// Compose's border draws inside the bounds: a stroke wider than half the smaller side fills
/// the box (`strokeWidth * 2 > size.minDimension`). compose-ui insets the rectangle by at most
/// half the side but keeps the full stroke width, so a thick border paints outside the node.
#[test]
fn a_thick_border_stays_inside_the_node() {
    let host = host(|| Leaf(Modifier.size(10.dp()).border(30.dp(), Color::BLACK)));
    for op in &host.display_list().ops {
        if let DrawOp::DrawRect(rect, paint) = op {
            let half = match paint.style {
                PaintStyle::Stroke(width) => width / 2.0,
                PaintStyle::Fill => 0.0,
            };
            assert!(
                rect.left - half >= 0.0 && rect.right + half <= 10.0,
                "paints outside 0..10: {}",
                host.display_list().dump()
            );
        }
    }
}

/// Two `Custom` elements of different types at the same place: the second must create its own
/// node (sound: checked, keeps passing).
#[test]
fn custom_elements_of_different_types_get_their_own_nodes() {
    struct A;
    struct B;
    struct Logged(&'static str);
    impl ModifierNode for Logged {
        fn on_release(&mut self, _: &mut ReleaseContext<'_>) {
            log(format!("detach {}", self.0));
        }
    }
    impl ModifierNodeElement for A {
        type Node = Logged;
        fn create(&self) -> Logged {
            log("create A");
            Logged("A")
        }
        fn update(&self, _: &mut Self::Node) {
            log("update A");
        }
        fn same(&self, _other: &Self) -> bool {
            true
        }
    }
    impl ModifierNodeElement for B {
        type Node = Logged;
        fn create(&self) -> Logged {
            log("create B");
            Logged("B")
        }
        fn update(&self, _: &mut Self::Node) {
            log("update B");
        }
        fn same(&self, _other: &Self) -> bool {
            true
        }
    }
    let second = mutable_state_of(false);
    let handle = second.handle();
    let mut host = host(move || {
        let modifier = if handle.get() { Modifier.then_custom(B) } else { Modifier.then_custom(A) };
        Leaf(modifier);
    });
    assert_eq!(take_log(), ["create A"]);
    second.set(true);
    host.frame();
    assert_eq!(take_log(), ["create B", "detach A"]);
}

// ---- Units and helpers -------------------------------------------------------------------------

/// Compose's `BiasAlignment.align` rounds with `fastRoundToInt` (`Math.round`: half up), so a
/// child 13 wide centered in 10 sits at round(-1.5) = -1. compose-ui's `f32::round` rounds half
/// away from zero: -2. The same goes for `Density::round_to_px` (-0.5 dp is 0 px in Compose).
#[test]
fn rounding_follows_kotlin_math_round() {
    let offset =
        Alignment::CENTER.align(IntSize::new(13, 13), IntSize::new(10, 10), LayoutDirection::Ltr);
    assert_eq!(offset, IntOffset::new(-1, -1));
    assert_eq!(Density::default().round_to_px((-0.5).dp()), 0);
}

/// Compose's `Constraints.offset(horizontal, vertical)` adds: `offset(-20, -5)` shrinks, as
/// `PaddingNode` calls it. compose-ui's subtracts, the opposite sign under the same name.
#[test]
fn constraints_offset_adds_as_in_compose() {
    let constraints = Constraints::new(10, 100, 0, 50);
    assert_eq!(constraints.offset(-20, -5), Constraints::new(0, 80, 0, 45));
}

// ---- Semantics ---------------------------------------------------------------------------------

/// Compose merges `Focused` with the default policy (`parentValue ?: childValue`): a merging
/// node whose focusable descendant has focus is focused in the merged tree. compose-ui's
/// `merge_child` drops `focused` (and `disabled`, `focusable`).
#[test]
fn a_merging_node_takes_its_descendants_focus() {
    let mut host = host(|| {
        Stack!(modifier = Modifier.semantics_with(true, |config| config.set_text("row")), {
            Leaf(Modifier.size(10.dp()).focusable());
        });
    });
    host.send_key(KeyEvent::down(Key::Tab));
    host.frame();
    assert_eq!(focused_count(&host), 1, "{}", host.semantics_tree(true).dump());
}

/// Compose takes a merging node's bounds from its outermost merging semantics modifier
/// (`findCoordinatorToGetBounds`): `testTag(…).padding(10).clickable { … }` has the clickable's
/// bounds, inside the padding. compose-ui takes the first semantics modifier's (the tag's).
#[test]
fn a_merging_nodes_bounds_are_its_merging_modifiers() {
    let host = host(|| {
        Leaf(Modifier.test_tag("tag").padding(10.dp()).clickable(|| {}).size(20.dp()));
    });
    let tree = host.semantics_tree(true);
    assert_eq!(tree.children[0].bounds, Rect::new(10.0, 10.0, 30.0, 30.0));
}

// ---- The applier: a randomized check -------------------------------------------------------------

/// Random inserts, removes and moves of keyed children: the tree's children follow the
/// composition's order and the node count returns to the baseline (sound: keeps passing).
#[test]
fn keyed_children_follow_random_edits() {
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, bound: usize) -> usize {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) % bound as u64) as usize
        }
    }
    let items: Rc<RefCell<Vec<u32>>> = Rc::new(RefCell::new(vec![]));
    let version = mutable_state_of(0u32);
    let (shared, handle) = (items.clone(), version.handle());
    let mut host = host(move || {
        handle.get();
        let list = shared.borrow().clone();
        Column!({
            for &item in &list {
                key(item, || Leaf(Modifier.size((1.dp(), (item as f32 + 1.0).dp()))));
            }
        });
    });
    let baseline = host.with_tree(|tree| tree.node_count());
    let mut random = Lcg(7);
    let mut next = 0u32;
    for round in 0..400 {
        {
            let mut list = items.borrow_mut();
            for _ in 0..1 + random.next(4) {
                match random.next(4) {
                    0 | 1 if list.len() < 12 => {
                        let at = random.next(list.len() + 1);
                        list.insert(at, next);
                        next += 1;
                    }
                    2 if !list.is_empty() => {
                        let at = random.next(list.len());
                        list.remove(at);
                    }
                    _ if list.len() > 1 => {
                        let from = random.next(list.len());
                        let item = list.remove(from);
                        let to = random.next(list.len() + 1);
                        list.insert(to, item);
                    }
                    _ => {}
                }
            }
        }
        version.set(round + 1);
        host.frame();
        let expected = items.borrow().clone();
        let heights: Vec<u32> = host.with_tree(|tree| {
            let column = tree.children_of(tree.root_id()).next().unwrap();
            tree.children_of(column)
                .map(|child| tree.bounds_in_root(child).height() as u32 - 1)
                .collect()
        });
        assert_eq!(heights, expected, "round {round}");
        assert_eq!(
            host.with_tree(|tree| tree.node_count()),
            baseline + expected.len(),
            "round {round}"
        );
    }
}

// ---- Pointers and keys ---------------------------------------------------------------------------

/// Compose's tap detection completes when the last pointer goes up (`waitForUpOrCancellation`
/// waits for every change to be up). `ClickableNode` has one `pressed` flag for all pointers:
/// a second finger's release completes the first finger's press.
#[test]
fn a_second_pointer_does_not_complete_the_first_pointers_click() {
    let mut host = host(|| Leaf(Modifier.size(50.dp()).clickable(|| log("click"))));
    let at = Offset::new(10.0, 10.0);
    let finger = |event: PointerEvent| event.with_pointer(PointerId(1), PointerType::Touch);
    host.send_pointer(PointerEvent::press(at));
    host.send_pointer(finger(PointerEvent::press(at)));
    host.send_pointer(finger(PointerEvent::release(at)));
    assert!(take_log().is_empty(), "the mouse button is still down");
    host.send_pointer(PointerEvent::release(at));
    assert_eq!(take_log(), ["click"]);
}

/// Compose's clickable tracks key presses per key (`currentKeyPressInteractions`): Enter and
/// Space held together click once each on release. `PressState::key_pressed` keeps one key, so
/// the second press forgets the first.
#[test]
fn enter_and_space_held_together_each_click() {
    let mut host = host(|| Leaf(Modifier.size(10.dp()).clickable(|| log("click"))));
    host.send_key(KeyEvent::down(Key::Tab));
    host.send_key(KeyEvent::down(Key::Enter));
    host.send_key(KeyEvent::down(Key::Space));
    host.send_key(KeyEvent::up(Key::Space));
    host.send_key(KeyEvent::up(Key::Enter));
    assert_eq!(take_log(), ["click", "click"]);
}

// ---- Memoized modifier closures --------------------------------------------------------------------

/// Capture-less closures in the branches of an `if`, and in a `bool::then` closure, at the same
/// place: each keeps its own callback (sound: keeps passing).
#[test]
fn branches_keep_their_own_modifier_closures() {
    #[composable]
    fn Switch(flag: MutableState<bool>) {
        let modifier = if flag.get() {
            Modifier.size(10.dp()).clickable(|| log("a"))
        } else {
            Modifier.size(10.dp()).clickable(|| log("b"))
        };
        Leaf(modifier);
        let other = flag.get().then(|| Modifier.clickable(|| log("c")));
        Leaf(other.unwrap_or(Modifier).size(10.dp()));
    }
    let flag = mutable_state_of(true);
    let handle = flag.handle();
    let mut host = host(move || Column!({ Switch(handle) }));
    click(&mut host, 5.0, 5.0);
    click(&mut host, 5.0, 15.0);
    flag.set(false);
    host.frame();
    click(&mut host, 5.0, 5.0);
    click(&mut host, 5.0, 15.0);
    flag.set(true);
    host.frame();
    click(&mut host, 5.0, 5.0);
    click(&mut host, 5.0, 15.0);
    assert_eq!(take_log(), ["a", "c", "b", "a", "c"]);
}
