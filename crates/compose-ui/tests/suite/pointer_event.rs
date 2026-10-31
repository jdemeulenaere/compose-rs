//! Pointer events with several changes and with buttons, as Compose's `PointerEvent` (D1657): one
//! platform event's changes together, each node hearing those that hit it.

use crate::support;

use std::cell::RefCell;
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::platform::*;
use compose_ui::*;
use support::*;

// ---- A change ------------------------------------------------------------------------------------

/// `x`, `y` and whether it's down.
type Place = (f32, f32, bool);

/// A change at `now` from `previous`.
fn change(previous: Place, now: Place, consumed: bool) -> PointerInputChange {
    PointerInputChange::new(
        PointerId(0),
        100,
        Offset::new(now.0, now.1),
        now.2,
        0,
        Offset::new(previous.0, previous.1),
        previous.2,
        consumed,
    )
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/PointerInputTest.kt, changedToDown_didNotChange_returnsFalse, changedToDown_changeNotConsumed_returnsTrue, changedToDown_changeNotConsumed_returnsFalse, changedToDownIgnoreConsumed_didNotChange_returnsFalse, changedToDownIgnoreConsumed_changedNotConsumed_returnsTrue, changedToDownIgnoreConsumed_changedConsumed_returnsTrue
#[test]
fn a_pointer_changed_to_down_only_when_it_was_up() {
    let (up, down) = ((0.0, 0.0, false), (0.0, 0.0, true));
    for (previous, now) in [(up, up), (down, up), (down, down)] {
        assert!(!change(previous, now, false).changed_to_down());
        assert!(!change(previous, now, false).changed_to_down_ignore_consumed());
    }
    assert!(change(up, down, false).changed_to_down());
    assert!(!change(up, down, true).changed_to_down());
    assert!(change(up, down, false).changed_to_down_ignore_consumed());
    assert!(change(up, down, true).changed_to_down_ignore_consumed());
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/PointerInputTest.kt, changedToUp_didNotChange_returnsFalse, changedToUp_changeNotConsumed_returnsTrue, changedToUp_changeNotConsumed_returnsFalse, changedToUpIgnoreConsumed_didNotChange_returnsFalse, changedToUpIgnoreConsumed_changedNotConsumed_returnsTrue, changedToUpIgnoreConsumed_changedConsumed_returnsTrue
#[test]
fn a_pointer_changed_to_up_only_when_it_was_down() {
    let (up, down) = ((0.0, 0.0, false), (0.0, 0.0, true));
    for (previous, now) in [(up, up), (up, down), (down, down)] {
        assert!(!change(previous, now, false).changed_to_up());
        assert!(!change(previous, now, false).changed_to_up_ignore_consumed());
    }
    assert!(change(down, up, false).changed_to_up());
    assert!(!change(down, up, true).changed_to_up());
    assert!(change(down, up, false).changed_to_up_ignore_consumed());
    assert!(change(down, up, true).changed_to_up_ignore_consumed());
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/PointerInputTest.kt, positionChange_didNotChange_returnsZeroOffset, positionChange_changedNotConsumed_returnsFullOffset, positionChange_changedFullConsumed_returnsZeroOffset, positionChangeIgnoreConsumed_didNotChange_returnsZeroOffset, positionChangeIgnoreConsumed_changedNotConsumed_returnsFullOffset, positionChangeIgnoreConsumed_changedFullConsumed_returnsFullOffset
#[test]
fn a_position_change_is_nothing_once_consumed() {
    let (from, to) = ((3.0, 4.0, true), (8.0, 16.0, true));
    assert_eq!(change(from, from, false).position_change(), Offset::ZERO);
    assert_eq!(change(from, from, false).position_change_ignore_consumed(), Offset::ZERO);
    assert_eq!(change(from, to, false).position_change(), Offset::new(5.0, 12.0));
    assert_eq!(change(from, to, true).position_change(), Offset::ZERO);
    assert_eq!(change(from, to, false).position_change_ignore_consumed(), Offset::new(5.0, 12.0));
    assert_eq!(change(from, to, true).position_change_ignore_consumed(), Offset::new(5.0, 12.0));
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/PointerInputTest.kt, positionChanged_didNotChange_returnsFalse, positionChanged_changedNotConsumed_returnsTrue, positionChanged_changedFullConsumed_returnsFalse, positionChangedIgnoreConsumed_didNotChange_returnsFalse, positionChangedIgnoreConsumed_changedNotConsumed_returnsTrue, positionChangedIgnoreConsumed_changedFullConsumed_returnsTrue
#[test]
fn a_position_changed_unless_consumed() {
    let (from, to) = ((3.0, 4.0, true), (8.0, 4.0, true));
    assert!(!change(from, from, false).position_changed());
    assert!(!change(from, from, false).position_changed_ignore_consumed());
    assert!(change(from, to, false).position_changed());
    assert!(!change(from, to, true).position_changed());
    assert!(change(from, to, false).position_changed_ignore_consumed());
    assert!(change(from, to, true).position_changed_ignore_consumed());
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/input/pointer/PointerInputTest.kt, consume_noChanges_returnsTrue, consumeAllChanges_nothingChanged_stillConsumed, consumeAllChanges_downChanged_downChangeConsumed, consumeAllChanges_movementChanged_movementFullyConsumed, consumeAllChanges_allChanged_allConsumed
#[test]
fn consuming_takes_whatever_changed_and_copies_share_it() {
    let from = (0.0, 0.0, false);
    for now in [(0.0, 0.0, false), (0.0, 0.0, true), (5.0, 0.0, false), (5.0, 0.0, true)] {
        let change = change(from, now, false);
        assert!(!change.is_consumed());
        let copy = change.clone();
        change.consume();
        assert!(change.is_consumed());
        assert!(copy.is_consumed());
        assert!(!change.changed_to_down());
        assert_eq!(change.position_change(), Offset::ZERO);
    }
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/input/pointer/PointerEvent.kt, isOutOfBounds
#[test]
fn a_touch_may_be_out_of_bounds_further_than_a_mouse() {
    let size = IntSize::new(10, 10);
    let padding = Size::new(5.0, 5.0);
    let at = |x, y| change((0.0, 0.0, true), (x, y, true), false);
    assert!(!at(10.0, 10.0).is_out_of_bounds(size));
    assert!(at(10.5, 0.0).is_out_of_bounds(size));
    assert!(at(0.0, -0.5).is_out_of_bounds(size));
    assert!(!at(14.0, -4.0).is_out_of_bounds_with(size, padding));
    assert!(at(15.5, 0.0).is_out_of_bounds_with(size, padding));
    let mut mouse = at(14.0, 0.0);
    mouse.pointer_type = PointerType::Mouse;
    assert!(mouse.is_out_of_bounds_with(size, padding));
}

// Compose: compose/ui/ui/src/androidMain/kotlin/androidx/compose/ui/input/pointer/PointerEvent.android.kt, calculatePointerEventType
#[test]
fn an_event_of_changes_finds_its_reason_in_them() {
    let (up, down) = ((0.0, 0.0, false), (0.0, 0.0, true));
    let kind = |changes: &[(Place, Place)]| {
        let changes = changes.iter().map(|it| change(it.0, it.1, false)).collect();
        PointerEvents::new(changes).kind
    };
    assert_eq!(kind(&[(down, down)]), PointerEventType::Move);
    assert_eq!(kind(&[(up, up)]), PointerEventType::Move);
    assert_eq!(kind(&[(down, down), (up, down)]), PointerEventType::Press);
    assert_eq!(kind(&[(down, down), (down, up)]), PointerEventType::Release);
}

// ---- Buttons and keys ----------------------------------------------------------------------------

/// What a node heard: which node, in which pass, and the event.
#[derive(Clone)]
struct Heard {
    node: &'static str,
    pass: PointerEventPass,
    event: PointerEvents,
}

type Record = Rc<RefCell<Vec<Heard>>>;

/// Takes whole events, and writes them down.
struct Recorder(&'static str, Record);

struct RecorderNode(&'static str, Record);

impl ModifierNodeElement for Recorder {
    type Node = RecorderNode;
    fn create(&self) -> RecorderNode {
        RecorderNode(self.0, self.1.clone())
    }
    fn update(&self, _: &mut RecorderNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for RecorderNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for RecorderNode {
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
        self.1.borrow_mut().push(Heard { node: self.0, pass, event: event.clone() });
    }
}

/// Hears a pointer's changes one at a time.
struct OneAtATime(&'static str, Rc<RefCell<Vec<String>>>);

struct OneAtATimeNode(&'static str, Rc<RefCell<Vec<String>>>);

impl ModifierNodeElement for OneAtATime {
    type Node = OneAtATimeNode;
    fn create(&self) -> OneAtATimeNode {
        OneAtATimeNode(self.0, self.1.clone())
    }
    fn update(&self, _: &mut OneAtATimeNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for OneAtATimeNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for OneAtATimeNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: PointerEventPass,
        _: IntSize,
        _: &mut EventContext<'_>,
    ) {
        if pass == PointerEventPass::Main
            && !matches!(event.kind, PointerEventType::Enter | PointerEventType::Exit)
        {
            let line = format!(
                "{} {:?} {} {} {}",
                self.0, event.kind, event.pointer.0, event.position.x, event.pressed
            );
            self.1.borrow_mut().push(line);
        }
    }
}

fn recorded(content: impl Fn(Record) + 'static) -> (UiHost, Record) {
    let record = Record::default();
    let seen = record.clone();
    (host(move || content(seen.clone())), record)
}

fn finger(id: u64, x: f32, y: f32) -> PointerEvent {
    PointerEvent::moved(Offset::new(x, y), true).with_pointer(PointerId(id), PointerType::Touch)
}

fn finger_down(id: u64, x: f32, y: f32) -> PointerEvent {
    PointerEvent::press(Offset::new(x, y)).with_pointer(PointerId(id), PointerType::Touch)
}

fn finger_up(id: u64, x: f32, y: f32) -> PointerEvent {
    PointerEvent::release(Offset::new(x, y)).with_pointer(PointerId(id), PointerType::Touch)
}

/// `node` then its pass, as the order things were heard in.
fn order(record: &Record) -> Vec<String> {
    record.borrow().iter().map(|it| format!("{} {:?}", it.node, it.pass)).collect()
}

fn ids(event: &PointerEvents) -> Vec<u64> {
    event.changes.iter().map(|it| it.id.0).collect()
}

fn presses(record: &Record, node: &str) -> Vec<Heard> {
    record.borrow().iter().filter(|it| it.node == node).cloned().collect()
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/PointerInputEventProcessorTest.kt, buttonsPressed
#[test]
fn an_event_tells_the_buttons_held_by_their_indices() {
    use PointerButtons as Buttons;
    let table = [
        (Buttons::PRIMARY, vec![0]),
        (Buttons::SECONDARY, vec![1]),
        (Buttons::TERTIARY, vec![2]),
        (Buttons(1 << 5), vec![0]),
        (Buttons(1 << 6), vec![1]),
        (Buttons::BACK, vec![3]),
        (Buttons::FORWARD, vec![4]),
        (Buttons(Buttons::PRIMARY.0 | Buttons::TERTIARY.0), vec![0, 2]),
        (Buttons(Buttons::BACK.0 | (1 << 5)), vec![0, 3]),
        (Buttons::NONE, vec![]),
    ];
    let (mut host, record) = recorded(|record| {
        Leaf(Modifier.size(50.dp()).then_custom(Recorder("node", record.clone())));
    });
    for (buttons, pressed) in table {
        host.send_pointer(PointerEvent::press(Offset::new(5.0, 5.0)).with_buttons(buttons));
        let heard = record.borrow().last().unwrap().event.buttons;
        assert_eq!(heard, buttons);
        assert_eq!(heard.is_primary_pressed(), pressed.contains(&0));
        assert_eq!(heard.is_secondary_pressed(), pressed.contains(&1));
        assert_eq!(heard.is_tertiary_pressed(), pressed.contains(&2));
        assert_eq!(heard.is_back_pressed(), pressed.contains(&3));
        assert_eq!(heard.is_forward_pressed(), pressed.contains(&4));
        assert_eq!(heard.are_any_pressed(), !pressed.is_empty());
        assert_eq!(heard.index_of_first_pressed(), pressed.first().map_or(-1, |it| *it));
        assert_eq!(heard.index_of_last_pressed(), pressed.last().map_or(-1, |it| *it));
        for index in 0..=10 {
            assert_eq!(heard.is_pressed(index as u32), pressed.contains(&index), "{index}");
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/PointerInputEventProcessorTest.kt, buttonsPressed
#[test]
fn a_button_without_a_purpose_has_an_index_past_the_stylus() {
    let buttons = PointerButtons(1 << 7);
    assert!(buttons.is_pressed(5));
    assert!(!buttons.is_pressed(7));
    assert_eq!(buttons.index_of_first_pressed(), 7);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/PointerInputEventProcessorTest.kt, metaState
#[test]
fn an_event_tells_the_modifier_keys_held() {
    let (mut host, record) = recorded(|record| {
        Leaf(Modifier.size(50.dp()).then_custom(Recorder("node", record.clone())));
    });
    #[derive(Default, PartialEq, Debug)]
    struct Keys {
        ctrl: bool,
        meta: bool,
        alt: bool,
        sym: bool,
        shift: bool,
        function: bool,
        caps: bool,
        scroll: bool,
        num: bool,
    }
    let table = [
        (0x1000, Keys { ctrl: true, ..Keys::default() }),
        (0x10000, Keys { meta: true, ..Keys::default() }),
        (0x2, Keys { alt: true, ..Keys::default() }),
        (0x4, Keys { sym: true, ..Keys::default() }),
        (0x1, Keys { shift: true, ..Keys::default() }),
        (0x8, Keys { function: true, ..Keys::default() }),
        (0x100000, Keys { caps: true, ..Keys::default() }),
        (0x400000, Keys { scroll: true, ..Keys::default() }),
        (0x200000, Keys { num: true, ..Keys::default() }),
        (0x1000 | 0x1 | 0x200000, Keys { ctrl: true, shift: true, num: true, ..Keys::default() }),
        (0, Keys::default()),
    ];
    for (state, expected) in table {
        let modifiers = PointerKeyboardModifiers(state);
        host.send_pointer(PointerEvent::press(Offset::ZERO).with_keyboard_modifiers(modifiers));
        let heard = record.borrow().last().unwrap().event.keyboard_modifiers;
        assert!(!heard.is_alt_graph_pressed());
        let keys = Keys {
            ctrl: heard.is_ctrl_pressed(),
            meta: heard.is_meta_pressed(),
            alt: heard.is_alt_pressed(),
            sym: heard.is_sym_pressed(),
            shift: heard.is_shift_pressed(),
            function: heard.is_function_pressed(),
            caps: heard.is_caps_lock_on(),
            scroll: heard.is_scroll_lock_on(),
            num: heard.is_num_lock_on(),
        };
        assert_eq!(keys, expected);
    }
}

#[test]
fn the_keys_of_a_desktop_are_those_bits() {
    let keys = KeyModifiers { shift: true, ctrl: true, alt: false, meta: true };
    let modifiers = PointerKeyboardModifiers::from(keys);
    assert!(modifiers.is_shift_pressed() && modifiers.is_ctrl_pressed());
    assert!(modifiers.is_meta_pressed() && !modifiers.is_alt_pressed());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/PointerInputEventProcessorTest.kt, pointerTypePassed
#[test]
fn each_change_says_what_kind_of_pointer_it_is() {
    let (mut host, record) = recorded(|record| {
        Leaf(Modifier.size(50.dp()).then_custom(Recorder("node", record.clone())));
    });
    let types = [
        PointerType::Touch,
        PointerType::Mouse,
        PointerType::Stylus,
        PointerType::Eraser,
        PointerType::Unknown,
    ];
    let events: Vec<PointerEvent> = types
        .iter()
        .enumerate()
        .map(|(index, kind)| {
            PointerEvent::press(Offset::new(index as f32, 0.0))
                .with_pointer(PointerId(index as u64), *kind)
        })
        .collect();
    host.send_pointers(&events);
    let heard = record.borrow().last().unwrap().event.clone();
    let told: Vec<PointerType> = heard.changes.iter().map(|it| it.pointer_type).collect();
    assert_eq!(told, types);
}

// ---- The dispatch --------------------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/PointerInputEventProcessorTest.kt, process_downMoveUp_convertedCorrectlyAndTraversesAllPassesInCorrectOrder
#[test]
fn down_move_and_up_become_changes_through_every_pass_in_order() {
    let (mut host, record) = recorded(|record| {
        Leaf(Modifier.padding(10.dp()).size(50.dp()).then_custom(Recorder("node", record.clone())));
    });
    let at = |millis: u64| millis * 1_000_000;
    let send = |host: &mut UiHost, event: PointerEvent, millis: u64| {
        host.send_pointer(event.with_pointer(PointerId(1), PointerType::Touch).at_time(at(millis)));
    };
    send(&mut host, PointerEvent::press(Offset::new(15.0, 15.0)), 1);
    send(&mut host, PointerEvent::moved(Offset::new(25.0, 35.0), true), 7);
    send(&mut host, PointerEvent::release(Offset::new(25.0, 35.0)), 9);
    let passes = [PointerEventPass::Initial, PointerEventPass::Main, PointerEventPass::Final];
    let heard = record.borrow();
    assert_eq!(heard.len(), 9);
    for (index, heard) in heard.iter().enumerate() {
        assert_eq!(heard.pass, passes[index % 3]);
        let kinds = [PointerEventType::Press, PointerEventType::Move, PointerEventType::Release];
        assert_eq!(heard.event.kind, kinds[index / 3]);
    }
    let down = &heard[0].event.changes[0];
    assert_eq!(down.position, Offset::new(5.0, 5.0));
    assert_eq!(down.previous_position, Offset::new(5.0, 5.0));
    assert_eq!((down.uptime_millis, down.previous_uptime_millis), (1, 1));
    assert!(down.pressed && !down.previous_pressed);
    let moved = &heard[3].event.changes[0];
    assert_eq!(moved.position, Offset::new(15.0, 25.0));
    assert_eq!(moved.previous_position, Offset::new(5.0, 5.0));
    assert_eq!((moved.uptime_millis, moved.previous_uptime_millis), (7, 1));
    assert!(moved.pressed && moved.previous_pressed);
    let up = &heard[6].event.changes[0];
    assert_eq!(up.position, Offset::new(15.0, 25.0));
    assert_eq!(up.previous_position, Offset::new(15.0, 25.0));
    assert_eq!((up.uptime_millis, up.previous_uptime_millis), (9, 7));
    assert!(!up.pressed && up.previous_pressed);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/PointerInputEventProcessorTest.kt, process_2DownOn2DifferentPointerNodes_hitAndDispatchInfoAreCorrect
#[test]
fn two_downs_on_two_nodes_each_tell_their_own() {
    let (mut host, record) = recorded(|record| {
        Row!({
            Leaf(Modifier.size(50.dp()).then_custom(Recorder("a", record.clone())));
            Leaf(Modifier.size(50.dp()).then_custom(Recorder("b", record.clone())));
        });
    });
    host.send_pointers(&[finger_down(1, 10.0, 10.0), finger_down(2, 70.0, 20.0)]);
    for (node, id, position) in
        [("a", 1, Offset::new(10.0, 10.0)), ("b", 2, Offset::new(20.0, 20.0))]
    {
        let heard = presses(&record, node);
        assert_eq!(heard.len(), 3);
        for heard in heard {
            assert_eq!(ids(&heard.event), [id]);
            assert_eq!(heard.event.kind, PointerEventType::Press);
            assert_eq!(heard.event.changes[0].position, position);
            assert_eq!(heard.event.changes[0].previous_position, position);
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/PointerInputEventProcessorTest.kt, process_3DownOnOverlappingPointerNodes_hitAndDispatchInfoAreCorrect
#[test]
fn three_downs_on_overlapping_nodes_go_to_the_one_on_top() {
    let (mut host, record) = recorded(|record| {
        Stack!({
            Leaf(Modifier.size(100.dp()).then_custom(Recorder("a", record.clone())));
            Leaf(
                Modifier
                    .offset(50.dp(), 50.dp())
                    .size(100.dp())
                    .then_custom(Recorder("b", record.clone())),
            );
            Leaf(
                Modifier
                    .offset(100.dp(), 100.dp())
                    .size(100.dp())
                    .then_custom(Recorder("c", record.clone())),
            );
        });
    });
    host.resize(IntSize::new(400, 400));
    host.frame();
    host.send_pointers(&[
        finger_down(0, 25.0, 25.0),
        finger_down(1, 75.0, 75.0),
        finger_down(2, 125.0, 125.0),
    ]);
    for (node, id) in [("a", 0), ("b", 1), ("c", 2)] {
        let heard = presses(&record, node);
        assert_eq!(heard.len(), 3);
        assert_eq!(ids(&heard[0].event), [id]);
        assert_eq!(heard[0].event.changes[0].position, Offset::new(25.0, 25.0));
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, dispatchChanges_2IndependentBranchesFromRoot_eventsSplitCorrectlyAndCallOrderCorrect
#[test]
fn two_independent_paths_each_go_through_their_passes_a_path_at_a_time() {
    let (mut host, record) = recorded(|record| {
        Row!({
            Leaf(
                Modifier
                    .size(50.dp())
                    .then_custom(Recorder("outer1", record.clone()))
                    .then_custom(Recorder("inner1", record.clone())),
            );
            Leaf(
                Modifier
                    .size(50.dp())
                    .then_custom(Recorder("outer2", record.clone()))
                    .then_custom(Recorder("inner2", record.clone())),
            );
        });
    });
    host.send_pointers(&[finger_down(1, 10.0, 10.0), finger_down(2, 60.0, 10.0)]);
    assert_eq!(
        order(&record),
        [
            "outer1 Initial",
            "inner1 Initial",
            "inner1 Main",
            "outer1 Main",
            "outer2 Initial",
            "inner2 Initial",
            "inner2 Main",
            "outer2 Main",
            "outer1 Final",
            "inner1 Final",
            "outer2 Final",
            "inner2 Final",
        ]
    );
    assert_eq!(ids(&presses(&record, "outer1")[0].event), [1]);
    let second = presses(&record, "inner2");
    assert_eq!(ids(&second[0].event), [2]);
    assert_eq!(second[0].event.changes[0].position, Offset::new(10.0, 10.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, dispatchChanges_2BranchesWithSharedParent_eventsSplitCorrectlyAndCallOrderCorrect, dispatchChanges_2BranchesWithSharedParent_changesAreUpdatedCorrectly
#[test]
fn a_parent_hears_both_changes_its_branches_one_each() {
    let (mut host, record) = recorded(|record| {
        Row!(modifier = Modifier.then_custom(Recorder("parent", record.clone())), {
            Leaf(Modifier.size(50.dp()).then_custom(Recorder("a", record.clone())));
            Leaf(Modifier.size(50.dp()).then_custom(Recorder("b", record.clone())));
        });
    });
    host.send_pointers(&[finger_down(1, 10.0, 10.0), finger_down(2, 60.0, 10.0)]);
    assert_eq!(
        order(&record),
        [
            "parent Initial",
            "a Initial",
            "a Main",
            "b Initial",
            "b Main",
            "parent Main",
            "parent Final",
            "a Final",
            "b Final",
        ]
    );
    let parent = presses(&record, "parent");
    assert_eq!(ids(&parent[0].event), [1, 2]);
    assert_eq!(parent[0].event.changes[1].position, Offset::new(60.0, 10.0));
    let branch = presses(&record, "b");
    assert_eq!(ids(&branch[0].event), [2]);
    assert_eq!(branch[0].event.changes[0].position, Offset::new(10.0, 10.0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, dispatchChanges_2PointersShareCompletePath_eventsDoNotSplitAndCallOrderCorrect, dispatchChanges_2PointersShareCompletePath_changesAreUpdatedCorrectly
#[test]
fn two_pointers_on_one_path_are_one_event() {
    let (mut host, record) = recorded(|record| {
        Leaf(
            Modifier
                .size(50.dp())
                .then_custom(Recorder("outer", record.clone()))
                .then_custom(Recorder("inner", record.clone())),
        );
    });
    host.send_pointers(&[finger_down(1, 10.0, 10.0), finger_down(2, 20.0, 10.0)]);
    assert_eq!(
        order(&record),
        [
            "outer Initial",
            "inner Initial",
            "inner Main",
            "outer Main",
            "outer Final",
            "inner Final",
        ]
    );
    for heard in record.borrow().iter() {
        assert_eq!(ids(&heard.event), [1, 2]);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, dispatchChanges_2BranchesWithSharedParent_changesAreUpdatedCorrectly
#[test]
fn what_one_node_consumes_of_a_shared_change_the_next_sees() {
    struct Consumer;
    struct ConsumerNode;
    impl ModifierNodeElement for Consumer {
        type Node = ConsumerNode;
        fn create(&self) -> ConsumerNode {
            ConsumerNode
        }
        fn update(&self, _: &mut ConsumerNode) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    impl ModifierNode for ConsumerNode {
        fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
            Some(self)
        }
    }
    impl PointerInputModifierNode for ConsumerNode {
        fn on_pointer_event(
            &mut self,
            event: &mut PointerEvent,
            pass: PointerEventPass,
            _: IntSize,
            _: &mut EventContext<'_>,
        ) {
            if pass == PointerEventPass::Main && event.pointer == PointerId(2) {
                event.consume();
            }
        }
    }
    let consumed: Rc<RefCell<Vec<(u64, bool)>>> = Rc::default();
    let seen = consumed.clone();
    let mut host = host(move || {
        let seen = seen.clone();
        Leaf(
            Modifier
                .size(50.dp())
                .pointer_input((), move |scope| {
                    let seen = seen.clone();
                    async move {
                        loop {
                            let event = scope.await_pointer_event(PointerEventPass::Main).await;
                            let mut seen = seen.borrow_mut();
                            seen.extend(event.changes.iter().map(|it| (it.id.0, it.is_consumed())));
                        }
                    }
                })
                .then_custom(Consumer),
        );
    });
    host.send_pointers(&[finger_down(1, 10.0, 10.0), finger_down(2, 20.0, 10.0)]);
    // The inner one's first, as the main pass goes; it consumes the second alone.
    assert_eq!(*consumed.borrow(), [(1, false), (2, true)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/PointerInputEventProcessorTest.kt, processCancel_downDownOnSamePimThenCancel_pimOnlyReceivesCorrectChangesThenCancel
#[test]
fn downs_on_one_node_then_a_cancel_tell_it_each_then_cancel_it_once() {
    struct Cancels(Rc<RefCell<Vec<String>>>);
    struct CancelsNode(Rc<RefCell<Vec<String>>>);
    impl ModifierNodeElement for Cancels {
        type Node = CancelsNode;
        fn create(&self) -> CancelsNode {
            CancelsNode(self.0.clone())
        }
        fn update(&self, _: &mut CancelsNode) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    impl ModifierNode for CancelsNode {
        fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
            Some(self)
        }
    }
    impl PointerInputModifierNode for CancelsNode {
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
                let mut log = self.0.borrow_mut();
                log.push(format!("{:?}", ids(event)));
                for change in &event.changes {
                    log.push(format!(
                        "{} at {},{} from {},{} {}, was {}",
                        change.id.0,
                        change.position.x,
                        change.position.y,
                        change.previous_position.x,
                        change.previous_position.y,
                        if change.pressed { "down" } else { "up" },
                        if change.previous_pressed { "down" } else { "up" },
                    ));
                }
            }
        }
        fn on_cancel_pointer_input(&mut self) {
            self.0.borrow_mut().push("cancel".into());
        }
    }
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = log.clone();
    let mut host = host(move || Leaf(Modifier.size(50.dp()).then_custom(Cancels(seen.clone()))));
    host.send_pointer(finger_down(1, 10.0, 10.0));
    host.send_pointers(&[finger(1, 10.0, 10.0), finger_down(2, 20.0, 10.0)]);
    host.with_tree(|tree| tree.cancel_pointers());
    assert_eq!(
        *log.borrow(),
        [
            "[1]",
            "1 at 10,10 from 10,10 down, was up",
            "[1, 2]",
            "1 at 10,10 from 10,10 down, was down",
            "2 at 20,10 from 20,10 down, was up",
            "cancel",
        ]
    );
    log.borrow_mut().clear();
    // Nothing of them is left: the same fingers come down anew.
    host.send_pointer(finger_down(1, 15.0, 10.0));
    assert_eq!(*log.borrow(), ["[1]", "1 at 15,10 from 15,10 down, was up"]);
    host.send_pointer(finger_up(1, 15.0, 10.0));
    log.borrow_mut().clear();
    // One finger's cancel is all of a node's, as Compose has no other: the first says it, and it
    // hears no more of the other.
    host.send_pointers(&[finger_down(1, 10.0, 10.0), finger_down(2, 20.0, 10.0)]);
    log.borrow_mut().clear();
    host.with_tree(|tree| tree.cancel_pointer(PointerId(1)));
    assert_eq!(*log.borrow(), ["cancel"]);
    host.send_pointer(finger(2, 22.0, 10.0));
    host.with_tree(|tree| tree.cancel_pointer(PointerId(2)));
    assert_eq!(*log.borrow(), ["cancel"]);
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/input/pointer/PointerInputEventProcessor.kt, process
#[test]
fn one_finger_of_two_coming_up_is_a_release_of_the_event() {
    let (mut host, record) = recorded(|record| {
        Leaf(Modifier.size(50.dp()).then_custom(Recorder("node", record.clone())));
    });
    host.send_pointer(finger_down(1, 10.0, 10.0));
    host.send_pointers(&[finger(1, 12.0, 10.0), finger_down(2, 20.0, 10.0)]);
    host.send_pointers(&[finger(1, 12.0, 10.0), finger_up(2, 20.0, 10.0)]);
    host.send_pointer(finger_up(1, 12.0, 10.0));
    let events: Vec<(PointerEventType, Vec<u64>)> = record
        .borrow()
        .iter()
        .filter(|it| it.pass == PointerEventPass::Main)
        .map(|it| (it.event.kind, ids(&it.event)))
        .collect();
    assert_eq!(
        events,
        [
            (PointerEventType::Press, vec![1]),
            (PointerEventType::Press, vec![1, 2]),
            (PointerEventType::Release, vec![1, 2]),
            (PointerEventType::Release, vec![1]),
        ]
    );
    // The resting finger of the press moved, with its own previous place.
    let heard = record.borrow();
    let resting = &heard[3].event.changes[0];
    assert_eq!(resting.position_change(), Offset::new(2.0, 0.0));
    assert!(!resting.changed_to_down());
    assert!(heard[3].event.changes[1].changed_to_down());
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/input/pointer/PointerInputEventProcessor.kt, PointerInputChangeEventProducer
#[test]
fn a_finger_a_host_says_nothing_of_is_in_the_event_at_rest_in_the_order_of_their_ids() {
    let (mut host, record) = recorded(|record| {
        Leaf(Modifier.size(50.dp()).then_custom(Recorder("node", record.clone())));
    });
    host.send_pointer(finger_down(2, 20.0, 10.0).at_time(1_000_000));
    host.send_pointer(finger_down(1, 10.0, 10.0).at_time(5_000_000));
    let event = record.borrow().last().unwrap().event.clone();
    assert_eq!(ids(&event), [1, 2]);
    assert_eq!(event.kind, PointerEventType::Press);
    assert!(event.changes[0].changed_to_down());
    let resting = &event.changes[1];
    assert!(!resting.changed_to_down_ignore_consumed() && resting.pressed);
    assert_eq!(resting.position_change(), Offset::ZERO);
    assert_eq!(resting.position, Offset::new(20.0, 10.0));
    assert_eq!((resting.previous_uptime_millis, resting.uptime_millis), (1, 5));
    assert_eq!(resting.pointer_type, PointerType::Touch);
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/input/pointer/PointerInputEventProcessor.kt, PointerInputChangeEventProducer
#[test]
fn a_finger_up_is_forgotten_a_mouse_never() {
    let (mut host, record) = recorded(|record| {
        Leaf(Modifier.size(50.dp()).then_custom(Recorder("node", record.clone())));
    });
    host.send_pointer(finger_down(1, 10.0, 10.0));
    host.send_pointer(finger_up(1, 10.0, 10.0));
    host.send_pointer(finger_down(1, 30.0, 30.0));
    let down = record.borrow().last().unwrap().event.changes[0].clone();
    assert_eq!(down.previous_position, Offset::new(30.0, 30.0));
    assert!(down.changed_to_down());
    assert!(!record.borrow().last().unwrap().event.buttons.are_any_pressed(), "a finger has none");
    host.send_pointer(finger_up(1, 30.0, 30.0));
    record.borrow_mut().clear();
    // A mouse is somewhere, and it was there.
    host.send_pointer(PointerEvent::moved(Offset::new(10.0, 10.0), false));
    host.send_pointer(PointerEvent::moved(Offset::new(14.0, 10.0), false));
    let moved = record.borrow().last().unwrap().event.changes[0].clone();
    assert_eq!(moved.position_change(), Offset::new(4.0, 0.0));
    assert!(!moved.pressed && !moved.previous_pressed);
}

#[test]
fn a_cancelled_finger_is_gone_from_the_events() {
    let (mut host, record) = recorded(|record| {
        Leaf(Modifier.size(50.dp()).then_custom(Recorder("node", record.clone())));
    });
    host.send_pointers(&[finger_down(1, 10.0, 10.0), finger_down(2, 20.0, 10.0)]);
    host.with_tree(|tree| tree.cancel_pointer(PointerId(1)));
    record.borrow_mut().clear();
    // The node's two went together: it hears a new press alone, its own.
    host.send_pointer(finger(2, 22.0, 10.0));
    assert!(record.borrow().is_empty());
    host.send_pointer(finger_down(3, 30.0, 10.0));
    assert_eq!(ids(&record.borrow().last().unwrap().event), [3]);
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/input/pointer/PointerEvent.kt, PointerInputChange
#[test]
fn a_change_was_where_its_pointer_was_in_the_node_as_it_is_now() {
    let inset = mutable_state_of(0.dp());
    let at = inset.handle();
    let (mut host, record) = recorded(move |record| {
        Leaf(
            Modifier.padding(at.get()).size(50.dp()).then_custom(Recorder("node", record.clone())),
        );
    });
    host.frame();
    host.send_pointer(finger_down(1, 20.0, 20.0));
    inset.set(5.dp());
    host.frame();
    host.send_pointer(finger(1, 21.0, 20.0));
    let moved = record.borrow().last().unwrap().event.changes[0].clone();
    assert_eq!(moved.position, Offset::new(16.0, 15.0));
    assert_eq!(moved.position_change(), Offset::new(1.0, 0.0));
}

// ---- A mouse's buttons ---------------------------------------------------------------------------

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/input/pointer/PointerEvent.kt, PointerButtons
#[test]
fn a_mouse_stays_with_what_its_first_button_pressed_until_the_last_is_up() {
    let (mut host, record) = recorded(|record| {
        Row!({
            Leaf(Modifier.size(50.dp()).then_custom(Recorder("a", record.clone())));
            Leaf(Modifier.size(50.dp()).then_custom(Recorder("b", record.clone())));
        });
    });
    let at = Offset::new(10.0, 10.0);
    let over_b = Offset::new(70.0, 10.0);
    let both = PointerButtons(PointerButtons::PRIMARY.0 | PointerButtons::SECONDARY.0);
    host.send_pointer(PointerEvent::press(at));
    host.send_pointer(PointerEvent::moved(over_b, true));
    host.send_pointer(PointerEvent::press(over_b).with_buttons(both));
    host.send_pointer(PointerEvent::release(over_b).with_buttons(PointerButtons::SECONDARY));
    host.send_pointer(PointerEvent::moved(over_b, true).with_buttons(PointerButtons::SECONDARY));
    assert!(presses(&record, "b").is_empty());
    host.send_pointer(PointerEvent::release(over_b).with_buttons(PointerButtons::NONE));
    let last = presses(&record, "a").last().unwrap().clone();
    assert_eq!(last.event.kind, PointerEventType::Release);
    assert!(last.event.changes[0].changed_to_up());
    assert!(!last.event.buttons.are_any_pressed());
    // Then it hovers, whatever's under it.
    let heard = presses(&record, "b");
    assert_eq!(heard.first().map(|it| it.event.kind), Some(PointerEventType::Enter));
}

#[test]
fn another_button_of_a_mouse_is_no_change_of_its_pointer_being_down() {
    let (mut host, record) = recorded(|record| {
        Leaf(Modifier.size(50.dp()).then_custom(Recorder("node", record.clone())));
    });
    let at = Offset::new(10.0, 10.0);
    let both = PointerButtons(PointerButtons::PRIMARY.0 | PointerButtons::SECONDARY.0);
    host.send_pointer(PointerEvent::press(at));
    host.send_pointer(PointerEvent::press(at).with_buttons(both));
    let heard = record.borrow().last().unwrap().event.clone();
    assert_eq!(heard.kind, PointerEventType::Press);
    assert!(!heard.changes[0].changed_to_down_ignore_consumed());
    assert!(heard.buttons.is_secondary_pressed());
}

#[test]
fn a_wheel_turned_during_a_drag_is_no_release_and_no_press() {
    let (mut host, record) = recorded(|record| {
        Leaf(Modifier.size(50.dp()).then_custom(Recorder("node", record.clone())));
    });
    let at = Offset::new(10.0, 10.0);
    host.send_pointer(PointerEvent::press(at));
    host.send_pointer(PointerEvent::scroll(at, Offset::new(0.0, 1.0)));
    let during = record.borrow().last().unwrap().event.clone();
    assert_eq!(during.kind, PointerEventType::Scroll);
    assert!(during.changes[0].pressed && !during.changes[0].changed_to_up());
    assert!(during.buttons.is_primary_pressed());
    host.send_pointer(PointerEvent::moved(Offset::new(12.0, 10.0), true));
    let after = record.borrow().last().unwrap().event.clone();
    assert!(!after.changes[0].changed_to_down_ignore_consumed());
}

#[test]
fn coming_over_a_node_is_not_its_press() {
    let (mut host, record) = recorded(|record| {
        Leaf(Modifier.size(50.dp()).then_custom(Recorder("node", record.clone())));
    });
    // The first thing the mouse does is press: it comes over the node then, then presses it.
    host.send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    let heard = record.borrow();
    assert_eq!(heard[0].event.kind, PointerEventType::Enter);
    assert!(!heard[0].event.changes[0].changed_to_down_ignore_consumed());
    assert_eq!(heard[3].event.kind, PointerEventType::Press);
    assert!(heard[3].event.changes[0].changed_to_down());
}

#[test]
fn a_node_taking_changes_one_at_a_time_hears_a_mouses_primary_button_alone() {
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = log.clone();
    let mut host = host(move || {
        Leaf(Modifier.size(50.dp()).then_custom(OneAtATime("node", seen.clone())));
    });
    let at = Offset::new(10.0, 10.0);
    let both = PointerButtons(PointerButtons::PRIMARY.0 | PointerButtons::SECONDARY.0);
    // The right button alone: nothing pressed, a move isn't pressing.
    host.send_pointer(PointerEvent::press(at).with_buttons(PointerButtons::SECONDARY));
    host.send_pointer(PointerEvent::moved(at, true).with_buttons(PointerButtons::SECONDARY));
    host.send_pointer(PointerEvent::release(at).with_buttons(PointerButtons::NONE));
    assert_eq!(*log.borrow(), ["node Move 0 10 false"]);
    log.borrow_mut().clear();
    // As Android sends a right click: its pointer's down until `ACTION_UP`, its button up before.
    let unknown = |buttons| PointerEvent {
        kind: PointerEventType::Unknown,
        buttons,
        ..PointerEvent::moved(at, true)
    };
    let held = |buttons| PointerEvent { buttons, ..PointerEvent::moved(at, true) };
    let right = PointerButtons::SECONDARY;
    host.send_pointer(PointerEvent { buttons: right, ..PointerEvent::press(at) });
    host.send_pointer(unknown(right));
    host.send_pointer(unknown(PointerButtons::NONE));
    host.send_pointer(PointerEvent::release(at));
    assert!(log.borrow().is_empty(), "{:?}", log.borrow());
    // The right held, the left comes in a move as Android has it, then its button event: one press.
    host.send_pointer(PointerEvent { buttons: right, ..PointerEvent::press(at) });
    host.send_pointer(held(both));
    host.send_pointer(unknown(both));
    host.send_pointer(unknown(right));
    host.send_pointer(PointerEvent::release(at));
    assert_eq!(*log.borrow(), ["node Press 0 10 true", "node Release 0 10 false"]);
    log.borrow_mut().clear();
    // A left click as Android sends it.
    let left = PointerButtons::PRIMARY;
    host.send_pointer(PointerEvent { buttons: left, ..PointerEvent::press(at) });
    host.send_pointer(unknown(left));
    host.send_pointer(unknown(PointerButtons::NONE));
    host.send_pointer(PointerEvent::release(at));
    assert_eq!(*log.borrow(), ["node Press 0 10 true", "node Release 0 10 false"]);
    log.borrow_mut().clear();
    // The left one, the right one coming and going meanwhile: only the left one's.
    host.send_pointer(PointerEvent::press(at));
    host.send_pointer(PointerEvent::press(at).with_buttons(both));
    host.send_pointer(PointerEvent::moved(at, true).with_buttons(both));
    host.send_pointer(PointerEvent::release(at).with_buttons(PointerButtons::PRIMARY));
    host.send_pointer(PointerEvent::release(at).with_buttons(PointerButtons::NONE));
    assert_eq!(
        *log.borrow(),
        ["node Press 0 10 true", "node Move 0 10 true", "node Release 0 10 false"]
    );
}

/// As a text's selection, primary-only on Android too (`awaitPrimaryFirstDown`): what follows the
/// host's flag says so.
#[test]
fn a_node_not_following_the_hosts_flag_hears_the_primary_button_whatever_it_says() {
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = log.clone();
    let mut host = host(move || {
        Leaf(Modifier.size(50.dp()).then_custom(OneAtATime("node", seen.clone())));
    });
    host.set_first_down_refers_to_primary_mouse_button_only(false);
    let at = Offset::new(10.0, 10.0);
    host.send_pointer(PointerEvent::press(at).with_buttons(PointerButtons::SECONDARY));
    host.send_pointer(PointerEvent::release(at).with_buttons(PointerButtons::NONE));
    assert!(log.borrow().is_empty());
}

#[test]
fn a_mouse_press_saying_no_button_is_its_only_button() {
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = log.clone();
    let mut host = host(move || {
        Leaf(Modifier.size(50.dp()).then_custom(OneAtATime("node", seen.clone())));
    });
    let at = Offset::new(10.0, 10.0);
    let none = PointerButtons::NONE;
    host.send_pointer(PointerEvent { buttons: none, ..PointerEvent::press(at) });
    host.send_pointer(PointerEvent { buttons: none, ..PointerEvent::moved(at, true) });
    host.send_pointer(PointerEvent::release(at));
    assert_eq!(
        *log.borrow(),
        ["node Press 0 10 true", "node Move 0 10 true", "node Release 0 10 false"]
    );
}

#[test]
fn a_node_taking_changes_one_at_a_time_hears_each_finger_that_changed() {
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = log.clone();
    let mut host = host(move || {
        Leaf(Modifier.size(50.dp()).then_custom(OneAtATime("node", seen.clone())));
    });
    host.send_pointer(finger_down(1, 10.0, 10.0));
    // The one resting while another comes down isn't a change.
    host.send_pointers(&[finger(1, 10.0, 10.0), finger_down(2, 20.0, 10.0)]);
    host.send_pointers(&[finger(1, 11.0, 10.0), finger(2, 21.0, 10.0)]);
    assert_eq!(
        *log.borrow(),
        [
            "node Press 1 10 true",
            "node Press 2 20 true",
            "node Move 1 11 true",
            "node Move 2 21 true",
        ]
    );
    log.borrow_mut().clear();
    // A host sending a finger at a time: only that one's heard, the other being in the event alone.
    host.send_pointer(finger(2, 25.0, 10.0));
    assert_eq!(*log.borrow(), ["node Move 2 25 true"]);
}

// ---- A block -------------------------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/SuspendingPointerInputFilterTest.kt, testAwaitSingleEvent, testAwaitSeveralEvents
#[test]
fn a_block_awaits_its_events_with_all_their_changes() {
    type Told = (PointerEventType, Vec<u64>, Vec<f32>);
    let log: Rc<RefCell<Vec<Told>>> = Rc::default();
    let seen = log.clone();
    let mut host = host(move || {
        let seen = seen.clone();
        Leaf(Modifier.size(50.dp()).pointer_input((), move |scope| {
            let seen = seen.clone();
            async move {
                for _ in 0..3 {
                    let event = scope.await_pointer_event(PointerEventPass::Main).await;
                    let xs = event.changes.iter().map(|it| it.position.x).collect();
                    seen.borrow_mut().push((event.kind, ids(&event), xs));
                }
            }
        }));
    });
    host.send_pointer(finger_down(1, 5.0, 5.0));
    host.send_pointers(&[finger(1, 10.0, 5.0), finger_down(2, 20.0, 5.0)]);
    host.send_pointers(&[finger(1, 10.0, 10.0), finger(2, 22.0, 5.0)]);
    host.send_pointers(&[finger(1, 11.0, 10.0), finger(2, 23.0, 5.0)]);
    assert_eq!(
        *log.borrow(),
        [
            (PointerEventType::Press, vec![1], vec![5.0]),
            (PointerEventType::Press, vec![1, 2], vec![10.0, 20.0]),
            (PointerEventType::Move, vec![1, 2], vec![10.0, 22.0]),
        ]
    );
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/input/pointer/SuspendingPointerInputFilter.kt, offerPointerEvent
/// A block is only resumed for the pass it waits for, and goes on to another in the same event.
#[test]
fn a_block_gets_each_pass_it_waits_for_however_it_goes_between_them() {
    let log: Rc<RefCell<Vec<(PointerEventPass, PointerEventType)>>> = Rc::default();
    let seen = log.clone();
    let mut host = host(move || {
        let seen = seen.clone();
        Leaf(Modifier.size(50.dp()).pointer_input((), move |scope| {
            let seen = seen.clone();
            async move {
                let passes = [
                    PointerEventPass::Final,
                    PointerEventPass::Initial,
                    PointerEventPass::Main,
                    PointerEventPass::Final,
                    PointerEventPass::Main,
                ];
                for pass in passes {
                    let event = scope.await_pointer_event(pass).await;
                    seen.borrow_mut().push((pass, event.kind));
                }
            }
        }));
    });
    host.send_pointer(finger_down(1, 5.0, 5.0));
    host.send_pointer(finger(1, 6.0, 5.0));
    host.send_pointer(finger_up(1, 6.0, 5.0));
    use PointerEventPass::{Final, Initial, Main};
    use PointerEventType::{Move, Press, Release};
    assert_eq!(
        *log.borrow(),
        [(Final, Press), (Initial, Move), (Main, Move), (Final, Move), (Main, Release)]
    );
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/input/pointer/SuspendingPointerInputFilter.kt, currentEvent, extendedTouchPadding
#[test]
fn a_block_knows_the_last_event_and_its_extended_padding() {
    type Told = (Vec<u64>, Size);
    let seen: Rc<RefCell<Vec<Told>>> = Rc::default();
    let noted = seen.clone();
    let mut host = host(move || {
        let noted = noted.clone();
        Leaf(Modifier.size(8.dp()).pointer_input((), move |scope| {
            let noted = noted.clone();
            async move {
                loop {
                    scope.await_pointer_event(PointerEventPass::Final).await;
                    let event = scope.current_event().expect("the last event");
                    noted.borrow_mut().push((ids(&event), scope.extended_touch_padding()));
                }
            }
        }));
    });
    host.send_pointer(finger_down(1, 5.0, 5.0));
    let heard = seen.borrow();
    assert_eq!(heard[0].0, [1]);
    // A minimum touch target of 48 around 8.
    assert_eq!(heard[0].1, Size::new(20.0, 20.0));
}
