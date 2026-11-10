//! Adversarial review tests for `w2-pointer` (`D2360–D2363`, `D2435–D2436`):
//! - `D2360`: single-dispatch hover move (`Exit`, `Enter`, `Move`) sharing consumption across
//!   sibling and disjoint branches, for both mouse and stylus, and `DelegatingNode` delegate order;
//! - `D2361`: held-mouse per-entry bounds (`0 <= local <= size` inclusive edges), offset child
//!   extending past parent, multi-button hold, post-release hover prefix pruning, and a held mouse
//!   recaptured after cancel (`D2121` + `D2361` -> `D2436`);
//! - `D2362` / `D2120`: `cancel_pointer` clearing cancelled entries from `hovered` and
//!   `stylus_hovered` (`D2435`);
//! - `D2363`: removing a `DelegatingNode` mid-gesture cancels its delegates in delegation order
//!   after inner entries, and moving content across a pass-through `Popup` isolates windows;
//! - Should-fix finding: `ReusableContent` reusing a hovered `Hoverable` suppresses re-entry on a
//!   subsequent mouse move inside the node (`reset_modifier_state` + `hover_move`).

use crate::support;

use std::cell::RefCell;
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::*;
use support::*;

type Log = Rc<RefCell<Vec<String>>>;

fn push(log: &Log, line: impl Into<String>) {
    log.borrow_mut().push(line.into());
}

fn mouse_hover(x: f32, y: f32) -> PointerEvent {
    PointerEvent::moved(Offset::new(x, y), false)
}

fn stylus_hover(x: f32, y: f32) -> PointerEvent {
    PointerEvent::moved(Offset::new(x, y), false).with_pointer(PointerId(9), PointerType::Stylus)
}

fn hover_at(stylus: bool, x: f32, y: f32) -> PointerEvent {
    if stylus { stylus_hover(x, y) } else { mouse_hover(x, y) }
}

fn finger_down(id: u64, x: f32, y: f32) -> PointerEvent {
    PointerEvent::press(Offset::new(x, y)).with_pointer(PointerId(id), PointerType::Touch)
}

fn finger(id: u64, x: f32, y: f32) -> PointerEvent {
    PointerEvent::moved(Offset::new(x, y), true).with_pointer(PointerId(id), PointerType::Touch)
}

fn finger_up(id: u64, x: f32, y: f32) -> PointerEvent {
    PointerEvent::release(Offset::new(x, y)).with_pointer(PointerId(id), PointerType::Touch)
}

/// Logs `<name> <pass> <kind> consumed=<bool>` and optionally consumes in `(pass, kind)`.
#[derive(Clone, Copy)]
struct PassRule {
    consume_on: Option<(PointerEventPass, PointerEventType)>,
}

struct FullProbe<const SLOT: usize = 0>(&'static str, Log, PassRule);

struct FullProbeNode(&'static str, Log, PassRule);

impl<const SLOT: usize> ModifierNodeElement for FullProbe<SLOT> {
    type Node = FullProbeNode;
    fn create(&self) -> FullProbeNode {
        FullProbeNode(self.0, self.1.clone(), self.2)
    }
    fn update(&self, node: &mut FullProbeNode) {
        node.0 = self.0;
        node.2 = self.2;
    }
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0 && self.2.consume_on == other.2.consume_on
    }
}

impl ModifierNode for FullProbeNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for FullProbeNode {
    fn takes_whole_events(&self) -> bool {
        true
    }
    fn on_pointer_events(
        &mut self,
        event: &PointerEvents,
        pass: PointerEventPass,
        _: IntSize,
        _: &mut EventContext<'_>,
    ) {
        let consumed = event.changes.iter().any(|c| c.is_consumed());
        assert_eq!(event.changes.len(), 1, "{} {pass:?} saw duplicate changes", self.0);
        push(&self.1, format!("{} {pass:?} {:?} consumed={consumed}", self.0, event.kind));
        if self.2.consume_on == Some((pass, event.kind)) {
            for change in &event.changes {
                change.consume();
            }
        }
    }
    fn on_cancel_pointer_input(&mut self) {
        push(&self.1, format!("{} cancel", self.0));
    }
}

fn full_probe<const SLOT: usize>(name: &'static str, log: &Log) -> Modifier {
    Modifier.then_custom(FullProbe::<SLOT>(name, log.clone(), PassRule { consume_on: None }))
}

fn consuming_probe(
    name: &'static str,
    log: &Log,
    pass: PointerEventPass,
    kind: PointerEventType,
) -> Modifier {
    Modifier.then_custom(FullProbe::<0>(
        name,
        log.clone(),
        PassRule { consume_on: Some((pass, kind)) },
    ))
}

/// Main-pass-only summary `<name> <kind> pressed=<bool> down=<bool>`.
struct MainProbe<const SLOT: usize = 0>(&'static str, Log);

struct MainProbeNode(&'static str, Log);

impl<const SLOT: usize> ModifierNodeElement for MainProbe<SLOT> {
    type Node = MainProbeNode;
    fn create(&self) -> MainProbeNode {
        MainProbeNode(self.0, self.1.clone())
    }
    fn update(&self, node: &mut MainProbeNode) {
        node.0 = self.0;
    }
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl ModifierNode for MainProbeNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for MainProbeNode {
    fn takes_whole_events(&self) -> bool {
        true
    }
    fn on_pointer_events(
        &mut self,
        event: &PointerEvents,
        pass: PointerEventPass,
        _: IntSize,
        _: &mut EventContext<'_>,
    ) {
        if pass == PointerEventPass::Main {
            let pressed = event.changes.iter().any(|c| c.pressed);
            let down = event.changes.iter().any(|c| c.changed_to_down_ignore_consumed());
            push(
                &self.1,
                format!(
                    "{} {:?} pressed={pressed} down={down} n={}",
                    self.0,
                    event.kind,
                    event.changes.len()
                ),
            );
        }
    }
    fn on_cancel_pointer_input(&mut self) {
        push(&self.1, format!("{} cancel", self.0));
    }
}

fn main_probe<const SLOT: usize>(name: &'static str, log: &Log) -> Modifier {
    Modifier.then_custom(MainProbe::<SLOT>(name, log.clone()))
}

/// D2360: a cross-branch hover move (both mouse and stylus) delivers `Exit`, `Enter` and `Move` in
/// one `HitBranches` dispatch, sharing a single `LiveChange` and consumption cell across both
/// sibling branches under a shared parent and completely disjoint root branches (`common == 0`).
#[test]
fn hover_move_shares_consumption_across_exited_entered_and_shared_parent_branches() {
    for stylus in [false, true] {
        // 1. Shared parent (`common == 1`): `child_a` consumes in `Main Exit`; `child_b` (`Initial`
        // and `Main Enter`), `parent` (`Main` and `Final Move`), and both `Final` passes see it
        // consumed, with exactly one change per event.
        let log = Log::default();
        let seen = log.clone();
        let mut host = host(move || {
            let inner = seen.clone();
            Row!(modifier = full_probe::<0>("parent", &seen), {
                Leaf(Modifier.size(50.dp()).then(consuming_probe(
                    "child_a",
                    &inner,
                    PointerEventPass::Main,
                    PointerEventType::Exit,
                )));
                Leaf(Modifier.size(50.dp()).then(full_probe::<0>("child_b", &inner)));
            });
        });
        host.send_pointer(hover_at(stylus, 10.0, 10.0));
        log.borrow_mut().clear();
        host.send_pointer(hover_at(stylus, 60.0, 10.0));
        assert_eq!(
            *log.borrow(),
            [
                "parent Initial Move consumed=false",
                "child_a Initial Exit consumed=false",
                "child_a Main Exit consumed=false",
                "child_b Initial Enter consumed=true",
                "child_b Main Enter consumed=true",
                "parent Main Move consumed=true",
                "parent Final Move consumed=true",
                "child_a Final Exit consumed=true",
                "child_b Final Enter consumed=true",
            ],
            "shared parent, stylus={stylus}"
        );

        // 2. Disjoint roots (`common == 0`): `box_a` consumes in `Initial Exit`; `box_b` sees it
        // consumed on `Initial Enter`.
        let log2 = Log::default();
        let seen2 = log2.clone();
        let mut host2 = support::host(move || {
            let seen2 = seen2.clone();
            Row!({
                Leaf(Modifier.size(50.dp()).then(consuming_probe(
                    "box_a",
                    &seen2,
                    PointerEventPass::Initial,
                    PointerEventType::Exit,
                )));
                Leaf(Modifier.size(50.dp()).then(full_probe::<0>("box_b", &seen2)));
            });
        });
        host2.send_pointer(hover_at(stylus, 10.0, 10.0));
        log2.borrow_mut().clear();
        host2.send_pointer(hover_at(stylus, 60.0, 10.0));
        assert_eq!(
            *log2.borrow(),
            [
                "box_a Initial Exit consumed=false",
                "box_a Main Exit consumed=true",
                "box_b Initial Enter consumed=true",
                "box_b Main Enter consumed=true",
                "box_a Final Exit consumed=true",
                "box_b Final Enter consumed=true",
            ],
            "disjoint roots, stylus={stylus}"
        );
    }
}

/// D2360 + D2361 + D2363: a `DelegatingNode` with multiple pointer-input delegates preserves
/// delegation order in `Initial`, `Main` and `Final` across unpressed hover `Enter`/`Move`/`Exit`,
/// across pressed mouse `Exit`/`Enter`, and when removed from a chain mid-gesture while an inner
/// modifier stays attached.
#[test]
fn delegating_node_keeps_delegation_order_across_hover_pressed_drag_and_mid_gesture_removal() {
    struct TwoDelegates(Log);
    struct TwoDelegatesNode {
        delegates: Delegates,
    }
    impl ModifierNodeElement for TwoDelegates {
        type Node = TwoDelegatesNode;
        fn create(&self) -> TwoDelegatesNode {
            let mut delegates = Delegates::new();
            delegates.delegate(FullProbeNode(
                "del_1",
                self.0.clone(),
                PassRule { consume_on: None },
            ));
            delegates.delegate(FullProbeNode(
                "del_2",
                self.0.clone(),
                PassRule { consume_on: None },
            ));
            TwoDelegatesNode { delegates }
        }
        fn update(&self, _: &mut TwoDelegatesNode) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    impl ModifierNode for TwoDelegatesNode {
        fn delegates(&mut self) -> Option<&mut Delegates> {
            Some(&mut self.delegates)
        }
    }

    let log = Log::default();
    let keep_delegates = mutable_state_of(true);
    let handle = keep_delegates.handle();
    let seen = log.clone();
    let mut host = host(move || {
        let on = handle.get();
        let seen = seen.clone();
        let mid = if on { Modifier.then_custom(TwoDelegates(seen.clone())) } else { Modifier };
        Row!({
            Leaf(
                Modifier
                    .size(50.dp())
                    .then(full_probe::<0>("outer", &seen))
                    .then(mid.clone())
                    .then(full_probe::<1>("inner", &seen)),
            );
            Leaf(Modifier.size(50.dp()).then(full_probe::<2>("sibling", &seen)));
        });
    });

    // Hover enter, then cross-branch hover exit onto `sibling`: `del_1` before `del_2` in every
    // pass.
    host.send_pointer(mouse_hover(10.0, 10.0));
    log.borrow_mut().clear();
    host.send_pointer(mouse_hover(60.0, 10.0));
    assert_eq!(
        *log.borrow(),
        [
            "outer Initial Exit consumed=false",
            "del_1 Initial Exit consumed=false",
            "del_2 Initial Exit consumed=false",
            "inner Initial Exit consumed=false",
            "inner Main Exit consumed=false",
            "del_1 Main Exit consumed=false",
            "del_2 Main Exit consumed=false",
            "outer Main Exit consumed=false",
            "sibling Initial Enter consumed=false",
            "sibling Main Enter consumed=false",
            "outer Final Exit consumed=false",
            "del_1 Final Exit consumed=false",
            "del_2 Final Exit consumed=false",
            "inner Final Exit consumed=false",
            "sibling Final Enter consumed=false",
        ]
    );

    // Move back, press, drag out (pressed Exit), drag back in (pressed Enter), then remove the
    // delegating entry mid-press while `outer` and `inner` stay in the chain.
    host.send_pointer(mouse_hover(10.0, 10.0));
    host.send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    log.borrow_mut().clear();
    host.send_pointer(PointerEvent::moved(Offset::new(60.0, 10.0), true));
    let main_exits: Vec<String> =
        log.borrow().iter().filter(|s| s.contains(" Main ")).cloned().collect();
    assert_eq!(
        main_exits,
        [
            "inner Main Exit consumed=false",
            "del_1 Main Exit consumed=false",
            "del_2 Main Exit consumed=false",
            "outer Main Exit consumed=false",
        ]
    );

    log.borrow_mut().clear();
    host.send_pointer(PointerEvent::moved(Offset::new(20.0, 10.0), true));
    let main_enters: Vec<String> =
        log.borrow().iter().filter(|s| s.contains(" Main ")).cloned().collect();
    assert_eq!(
        main_enters,
        [
            "inner Main Enter consumed=false",
            "del_1 Main Enter consumed=false",
            "del_2 Main Enter consumed=false",
            "outer Main Enter consumed=false",
        ]
    );

    log.borrow_mut().clear();
    keep_delegates.set(false);
    host.frame();
    assert_eq!(*log.borrow(), ["inner cancel", "del_1 cancel", "del_2 cancel"]);

    // `outer` stays on the captured path; `inner` was below the removed delegating entry and was
    // cut off.
    log.borrow_mut().clear();
    host.send_pointer(PointerEvent::moved(Offset::new(25.0, 10.0), true));
    assert_eq!(
        *log.borrow(),
        [
            "outer Initial Move consumed=false",
            "outer Main Move consumed=false",
            "outer Final Move consumed=false",
        ]
    );
}

/// D2361: held mouse over an offset inner layer (`outer` 0..50, `inner` 30..80) and a bystander
/// (`80..120`):
/// 1. Exact right edge `x == 50.0` stays inside `outer` (`holds` includes edges, as
///    `!isOutOfBounds`).
/// 2. At `x == 60.0`, `outer` hears pressed `Exit` while `inner` hears pressed `Move` (single
///    change `n=1`).
/// 3. Multi-button swap (`Secondary` down, `Primary` up) keeps the capture without entering
///    `bystander`.
/// 4. Releasing the last button at `x == 60.0` (outside `outer`, inside `inner`) drops `outer` and
///    its descendant `inner` from `hovered` (`HitPathTracker.cleanUpHits`), so the next unpressed
///    move at `(62, 10)` enters `inner` fresh.
#[test]
fn held_mouse_across_edge_and_offset_bounds_tracks_each_entry_and_prunes_from_first_outside_at_release()
 {
    let log = Log::default();
    let seen = log.clone();
    let mut host = host(move || {
        let seen = seen.clone();
        Row!({
            Leaf(
                Modifier
                    .size(50.dp())
                    .then(main_probe::<0>("outer", &seen))
                    .offset(30.dp(), 0.dp())
                    .size(50.dp())
                    .then(main_probe::<1>("inner", &seen)),
            );
            Leaf(
                Modifier
                    .offset(30.dp(), 0.dp())
                    .size(40.dp())
                    .then(main_probe::<2>("bystander", &seen)),
            );
        });
    });

    // Press at (40, 10): inside both `outer` [0, 50) and `inner` [30, 80).
    host.send_pointer(PointerEvent::press(Offset::new(40.0, 10.0)));
    log.borrow_mut().clear();

    // Exact right edge of `outer` (x = 50.0): `holds` includes `50.0 <= 50.0`, so both stay Move.
    host.send_pointer(PointerEvent::moved(Offset::new(50.0, 10.0), true));
    assert_eq!(
        *log.borrow(),
        ["inner Move pressed=true down=false n=1", "outer Move pressed=true down=false n=1"]
    );

    // Move to (60, 10): outside `outer`, still inside `inner`.
    log.borrow_mut().clear();
    host.send_pointer(PointerEvent::moved(Offset::new(60.0, 10.0), true));
    assert_eq!(
        *log.borrow(),
        ["inner Move pressed=true down=false n=1", "outer Exit pressed=true down=false n=1"]
    );

    // Multi-button swap over `bystander` (x = 90) and back to (60, 10): `bystander` hears nothing.
    let both = PointerButtons(PointerButtons::PRIMARY.0 | PointerButtons::SECONDARY.0);
    log.borrow_mut().clear();
    host.send_pointer(PointerEvent::moved(Offset::new(90.0, 10.0), true));
    host.send_pointer(PointerEvent::press(Offset::new(90.0, 10.0)).with_buttons(both));
    host.send_pointer(
        PointerEvent::release(Offset::new(90.0, 10.0)).with_buttons(PointerButtons::SECONDARY),
    );
    host.send_pointer(
        PointerEvent::moved(Offset::new(60.0, 10.0), true).with_buttons(PointerButtons::SECONDARY),
    );
    assert!(
        !log.borrow().iter().any(|s| s.starts_with("bystander")),
        "bystander heard during held drag: {:?}",
        log.borrow()
    );
    assert_eq!(
        *log.borrow(),
        [
            "inner Exit pressed=true down=false n=1",
            "outer Move pressed=true down=false n=1",
            "inner Press pressed=true down=false n=1",
            "outer Press pressed=true down=false n=1",
            "inner Release pressed=true down=false n=1",
            "outer Release pressed=true down=false n=1",
            "inner Enter pressed=true down=false n=1",
            "outer Move pressed=true down=false n=1",
        ]
    );

    // Final release at (60, 10): outside `outer` and inside `inner`; `cleanUpHits` drops `outer`
    // and its subtree, so `inner` enters again on the next unpressed move at (62, 10).
    log.borrow_mut().clear();
    host.send_pointer(
        PointerEvent::release(Offset::new(60.0, 10.0)).with_buttons(PointerButtons::NONE),
    );
    host.send_pointer(mouse_hover(62.0, 10.0));
    assert_eq!(
        *log.borrow(),
        [
            "inner Release pressed=false down=false n=1",
            "outer Release pressed=false down=false n=1",
            "inner Enter pressed=false down=false n=1",
        ]
    );
}

/// D2435 (revises D2120, D2362): `cancel_pointer` cancels every entry of its pointer's path, so
/// any matching entry in `hovered` or `stylus_hovered` must also be forgotten — both when the
/// mouse itself is cancelled (`cancel_pointer(PointerId(0))`, as `MouseInjectionScope::cancel` and
/// `PanStart` while pressed call) and when a touch finger sharing a hovered node is cancelled.
#[test]
#[ignore = "review: cancel_pointer leaves cancelled entries in hovered and stylus_hovered so the next move sends Move instead of Enter"]
fn single_pointer_cancel_clears_cancelled_entries_from_mouse_and_stylus_hover() {
    // 1. Mouse pressed on a `ClickableNode` + probe, then `cancel_pointer(PointerId(0))`: hover
    // and press end, and the next unpressed move inside re-enters hover.
    let log = Log::default();
    let source = MutableInteractionSource::new();
    let clickable_source = source.clone();
    let seen = log.clone();
    let mut host = host(move || {
        Leaf(
            Modifier
                .size(50.dp())
                .clickable_with(
                    ClickableOptions {
                        interaction_source: Some(clickable_source.clone()),
                        ..ClickableOptions::DEFAULT
                    },
                    || {},
                )
                .then(main_probe::<0>("box", &seen)),
        );
    });
    host.send_pointer(mouse_hover(10.0, 10.0));
    host.frame();
    assert!(source.is_hovered());
    host.send_pointer(
        PointerEvent::press(Offset::new(10.0, 10.0)).with_buttons(PointerButtons::PRIMARY),
    );
    host.frame();
    assert!(source.is_pressed());

    log.borrow_mut().clear();
    host.with_tree(|tree| tree.cancel_pointer(PointerId(0)));
    assert!(!source.is_hovered());
    host.frame();
    assert!(!source.is_pressed());

    host.send_pointer(mouse_hover(15.0, 10.0));
    host.frame();
    assert_eq!(*log.borrow(), ["box cancel", "box Enter pressed=false down=false n=1"]);
    assert!(source.is_hovered());

    // 2. Touch finger cancelled on a node hovered by mouse or stylus.
    for stylus in [false, true] {
        let log = Log::default();
        let source = MutableInteractionSource::new();
        let hover_source = source.clone();
        let seen = log.clone();
        let mut host = support::host(move || {
            Leaf(
                Modifier
                    .size(50.dp())
                    .hoverable(hover_source.clone())
                    .then(main_probe::<0>("box", &seen)),
            );
        });
        host.send_pointer(hover_at(stylus, 10.0, 10.0));
        host.frame();
        assert!(source.is_hovered(), "stylus={stylus}");

        host.send_pointer(finger_down(2, 10.0, 10.0));
        log.borrow_mut().clear();
        host.with_tree(|tree| tree.cancel_pointer(PointerId(2)));
        assert!(!source.is_hovered(), "stylus={stylus}");

        host.send_pointer(hover_at(stylus, 15.0, 10.0));
        host.frame();
        assert_eq!(
            *log.borrow(),
            ["box cancel", "box Enter pressed=false down=false n=1"],
            "stylus={stylus}"
        );
        assert!(source.is_hovered(), "stylus={stylus}");
    }
}

/// D2436 (revises D2121, D2361): when a held mouse is cancelled (`cancel_pointers` or
/// `cancel_pointer(PointerId(0))`) and keeps moving with its button still down, the recapturing
/// move (`D2121`) enters the recaptured path (`Enter` with `down=true`), subsequent moves inside
/// stay `Move` (no spurious mid-box `Enter`), and dragging outside delivers pressed `Exit`.
#[test]
#[ignore = "review: a held mouse recaptured after cancel skips follow_pressed_hover on the recapture move"]
fn held_mouse_recaptured_after_cancel_enters_on_recapture_moves_inside_and_exits_when_dragged_out()
{
    for single in [false, true] {
        let log = Log::default();
        let source = MutableInteractionSource::new();
        let clickable_source = source.clone();
        let seen = log.clone();
        let mut host = host(move || {
            Leaf(
                Modifier
                    .size(50.dp())
                    .clickable_with(
                        ClickableOptions {
                            interaction_source: Some(clickable_source.clone()),
                            ..ClickableOptions::DEFAULT
                        },
                        || {},
                    )
                    .then(main_probe::<0>("box", &seen)),
            );
        });

        host.send_pointer(
            PointerEvent::press(Offset::new(10.0, 10.0)).with_buttons(PointerButtons::PRIMARY),
        );
        host.frame();
        assert!(source.is_hovered() && source.is_pressed(), "single={single}");

        if single {
            host.with_tree(|tree| tree.cancel_pointer(PointerId(0)));
        } else {
            host.with_tree(|tree| tree.cancel_pointers());
        }
        host.frame();
        assert!(!source.is_hovered() && !source.is_pressed(), "single={single}");

        log.borrow_mut().clear();
        let held = |x: f32| {
            PointerEvent::moved(Offset::new(x, 10.0), true).with_buttons(PointerButtons::PRIMARY)
        };
        // Recapture at (12, 10) inside `box`, move to (15, 10) inside `box`, drag out to (80, 10),
        // then release at (80, 10).
        host.send_pointer(held(12.0));
        host.frame();
        assert!(source.is_hovered() && source.is_pressed(), "recaptured single={single}");

        host.send_pointer(held(15.0));
        host.send_pointer(held(80.0));
        host.frame();
        assert!(!source.is_hovered() && !source.is_pressed(), "exited single={single}");

        host.send_pointer(
            PointerEvent::release(Offset::new(80.0, 10.0)).with_buttons(PointerButtons::NONE),
        );
        assert_eq!(
            *log.borrow(),
            [
                "box Enter pressed=true down=true n=1",
                "box Move pressed=true down=false n=1",
                "box Exit pressed=true down=false n=1",
                "box Release pressed=false down=false n=1",
            ],
            "single={single}"
        );
    }
}

/// D2363 + D1924: when an outside press dismisses a pass-through `Popup` and in the same frame
/// `movable_content_of` moves a child out of a second `Popup` into the main window (or vice versa),
/// `hit_window_of` resolves each detaching node's pre-removal window (`parent_for_callbacks`) and
/// leaves the surviving main-window gesture intact.
#[test]
fn moving_content_out_of_a_popup_while_dismissing_a_pass_through_popup_keeps_main_gesture_alive() {
    let log = Log::default();
    let in_popup = mutable_state_of(true);
    let handle = in_popup.handle();
    let dismiss = in_popup.handle();
    let moved_log = log.clone();
    let main_log = log.clone();
    let mut host = host(move || {
        let movable = remember(|| {
            let moved_log = moved_log.clone();
            movable_content_of(move || {
                Leaf(Modifier.size(20.dp()).then(main_probe::<0>("moved", &moved_log)));
            })
        });
        let open = handle.get();
        let main_log = main_log.clone();
        Stack!(modifier = Modifier.size(100.dp()), {
            if open {
                let movable = movable.clone();
                Popup!(
                    on_dismiss_request = Some(move || dismiss.set(false)),
                    properties = PopupProperties::default(),
                    {
                        movable();
                    }
                );
            } else {
                movable();
            }
            Leaf(
                Modifier
                    .offset(40.dp(), 40.dp())
                    .size(40.dp())
                    .then(main_probe::<1>("beneath", &main_log)),
            );
        });
    });

    // Hover `moved` inside the popup at (10, 10), then touch-press `beneath` outside the popup at
    // (50, 50): dismisses the popup and moves `moved` into the main window.
    host.send_pointer(mouse_hover(10.0, 10.0));
    log.borrow_mut().clear();
    host.send_pointer(finger_down(1, 50.0, 50.0));
    assert!(!in_popup.get());
    host.frame();
    assert_eq!(
        *log.borrow(),
        [
            "beneath Press pressed=true down=true n=1",
            "moved cancel",
            "moved Enter pressed=false down=false n=1",
        ],
        "moved in popup cancels and re-enters under resting mouse in main window while beneath stays alive"
    );

    log.borrow_mut().clear();
    host.send_pointer(finger(1, 52.0, 50.0));
    host.send_pointer(finger_up(1, 52.0, 50.0));
    assert_eq!(
        *log.borrow(),
        [
            "beneath Move pressed=true down=false n=1",
            "beneath Release pressed=false down=false n=1",
        ]
    );
}

/// Should-fix (`tree.rs::reset_modifier_state` + `dispatch.rs::hover_move`): when `ReusableContent`
/// reuses a hovered `Hoverable`, `on_reset` ends the hover (`!is_hovered()`), and a resting frame
/// does not re-enter it (`HoverableTest.hoverableTest_interactionSource_resetWhenReused`). Moving
/// the mouse inside the reused node afterwards must deliver `Enter` again (Compose's `resetState`
/// detached the node from `HitPathTracker`); currently `reset_modifier_state` bumps `entry.version`
/// in `hovered`, so `hover_move` sees the same path and sends `Move` instead of `Enter`.
#[test]
#[ignore = "review: reset_modifier_state keeps reused entries in hovered with the new chain_version, so moving the mouse inside a reused Hoverable sends Move instead of Enter"]
fn a_hoverable_reused_while_hovered_enters_again_on_the_next_mouse_move() {
    let source = MutableInteractionSource::new();
    let key = mutable_state_of(true);
    let handle = key.handle();
    let given = source.clone();
    let mut host = host(move || {
        let given = given.clone();
        ReusableContent(handle.get(), move || {
            Leaf(Modifier.size(50.dp()).hoverable(given.clone()));
        });
    });

    host.send_pointer(mouse_hover(10.0, 10.0));
    host.frame();
    assert!(source.is_hovered());

    key.set(false);
    host.frame();
    assert!(!source.is_hovered(), "resting after reuse stays unhovered");

    host.send_pointer(mouse_hover(20.0, 10.0));
    host.frame();
    assert!(source.is_hovered(), "moving inside the reused node must re-enter hover");
}
