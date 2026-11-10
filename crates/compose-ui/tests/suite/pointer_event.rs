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

// ---- HitPathTracker's dispatch, through the host ------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, dispatchChanges_noNodes_doesNotCrash, dispatchChanges_noNodes_nothingChanges
/// A press on no pointer input node is told to none.
#[test]
fn a_press_on_no_node_is_told_to_none() {
    let (mut host, record) = recorded(|record| {
        Leaf(Modifier.size(50.dp()).then_custom(Recorder("node", record.clone())));
    });
    host.send_pointer(finger_down(0, 80.0, 80.0));
    assert!(record.borrow().is_empty());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, dispatchChanges_hitResultHasSingleMatch_pointerInputHandlerCalled
/// One node on the path hears the press in each pass, in order.
#[test]
fn one_node_hears_a_press_in_each_pass() {
    let (mut host, record) = recorded(|record| {
        Leaf(Modifier.size(50.dp()).then_custom(Recorder("node", record.clone())));
    });
    host.send_pointer(finger_down(13, 10.0, 10.0));
    let heard = record.borrow();
    let passes = [PointerEventPass::Initial, PointerEventPass::Main, PointerEventPass::Final];
    assert_eq!(heard.len(), 3);
    for (heard, pass) in heard.iter().zip(passes) {
        assert_eq!(heard.pass, pass);
        assert_eq!(heard.event.kind, PointerEventType::Press);
        assert_eq!(ids(&heard.event), [13]);
        assert_eq!(heard.event.changes[0].position, Offset::new(10.0, 10.0));
        assert!(heard.event.changes[0].changed_to_down());
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, dispatchChanges_hitResultHasMultipleMatches_pointerInputHandlersCalledInCorrectOrder, dispatchChanges_hasDownAndUpPath_pointerInputHandlersCalledInCorrectOrder
/// Three nodes on one path: down the path in the initial pass, up it in the main one, down it
/// again in the final one, each with the press.
#[test]
fn three_nodes_on_a_path_hear_down_then_up_then_down() {
    let (mut host, record) = recorded(|record| {
        Leaf(
            Modifier
                .size(50.dp())
                .then_custom(Recorder("pif1", record.clone()))
                .then_custom(Recorder("pif2", record.clone()))
                .then_custom(Recorder("pif3", record.clone())),
        );
    });
    host.send_pointer(finger_down(13, 10.0, 10.0));
    assert_eq!(
        order(&record),
        [
            "pif1 Initial",
            "pif2 Initial",
            "pif3 Initial",
            "pif3 Main",
            "pif2 Main",
            "pif1 Main",
            "pif1 Final",
            "pif2 Final",
            "pif3 Final",
        ]
    );
    for heard in record.borrow().iter() {
        assert_eq!(heard.event.kind, PointerEventType::Press);
        assert_eq!(ids(&heard.event), [13]);
    }
}

/// Compose's `PointerInputNodeMock` with a `pointerEventHandler`: takes whole events, writes down
/// each change as it comes (its node, pass, pointer, and whether it's consumed), then consumes
/// those `consumes` picks.
struct Consuming(&'static str, Rc<RefCell<Vec<String>>>, fn(&PointerInputChange) -> bool);

struct ConsumingNode(&'static str, Rc<RefCell<Vec<String>>>, fn(&PointerInputChange) -> bool);

impl ModifierNodeElement for Consuming {
    type Node = ConsumingNode;
    fn create(&self) -> ConsumingNode {
        ConsumingNode(self.0, self.1.clone(), self.2)
    }
    fn update(&self, _: &mut ConsumingNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for ConsumingNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for ConsumingNode {
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
        for change in &event.changes {
            let consumed = if change.is_consumed() { "consumed" } else { "not consumed" };
            self.1.borrow_mut().push(format!("{} {pass:?} {} {consumed}", self.0, change.id.0));
            if (self.2)(change) {
                change.consume();
            }
        }
    }
}

fn consuming(name: &'static str, log: &Rc<RefCell<Vec<String>>>) -> Consuming {
    Consuming(name, log.clone(), |_| true)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, dispatchChanges_hitResultHasSingleMatch_changesAreUpdatedCorrectly
/// What a node consumes of a press stays consumed: the node hears it consumed in the final pass.
#[test]
fn a_press_consumed_by_its_node_stays_consumed() {
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = log.clone();
    let mut host = host(move || {
        let consumes_presses =
            Consuming("pif1", seen.clone(), |it| it.pressed != it.previous_pressed);
        Leaf(Modifier.size(50.dp()).then_custom(consumes_presses));
    });
    host.send_pointer(finger_down(13, 10.0, 10.0));
    assert_eq!(
        *log.borrow(),
        ["pif1 Initial 13 not consumed", "pif1 Main 13 consumed", "pif1 Final 13 consumed"]
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, dispatchChanges_hitResultHasMultipleMatchesAndDownAndUpPaths_changesAreUpdatedCorrectly
/// Three nodes consuming all: the first hears the move unconsumed, every one after it consumed.
#[test]
fn a_move_consumed_on_a_path_is_consumed_for_the_rest() {
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = log.clone();
    let mut host = host(move || {
        Leaf(
            Modifier
                .size(50.dp())
                .then_custom(consuming("pif1", &seen))
                .then_custom(consuming("pif2", &seen))
                .then_custom(consuming("pif3", &seen)),
        );
    });
    host.send_pointer(finger_down(13, 10.0, 10.0));
    log.borrow_mut().clear();
    // Compose's `down(13).moveTo(10, 0f, 0f)`: a move in time, not in place.
    host.send_pointer(finger(13, 10.0, 10.0));
    assert_eq!(
        *log.borrow(),
        [
            "pif1 Initial 13 not consumed",
            "pif2 Initial 13 consumed",
            "pif3 Initial 13 consumed",
            "pif3 Main 13 consumed",
            "pif2 Main 13 consumed",
            "pif1 Main 13 consumed",
            "pif1 Final 13 consumed",
            "pif2 Final 13 consumed",
            "pif3 Final 13 consumed",
        ]
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, dispatchChanges_2IndependentBranchesFromRoot_changesAreUpdatedCorrectly
/// Two paths of two nodes, each pointer moved: what each path's first node consumes, its second
/// hears consumed; the other path's pointer isn't touched.
#[test]
fn moves_consumed_on_two_independent_paths_stay_on_their_own() {
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = log.clone();
    let mut host = host(move || {
        let moved = |name| {
            Consuming(name, seen.clone(), |it: &PointerInputChange| {
                it.position_change() != Offset::ZERO
            })
        };
        let first = Modifier.size(50.dp()).then_custom(moved("pif1")).then_custom(moved("pif2"));
        let second = Modifier
            .size(50.dp())
            .then_custom(consuming("pif3", &seen))
            .then_custom(consuming("pif4", &seen));
        Row!({
            Leaf(first.clone());
            Leaf(second.clone());
        });
    });
    host.send_pointers(&[finger_down(3, 10.0, 0.0), finger_down(5, 60.0, 0.0)]);
    log.borrow_mut().clear();
    host.send_pointers(&[finger(3, 10.0, 30.0), finger(5, 60.0, 30.0)]);
    let of = |names: [&str; 2]| -> Vec<String> {
        log.borrow()
            .iter()
            .filter(|line| names.iter().any(|name| line.starts_with(name)))
            .filter(|line| !line.contains("Final"))
            .cloned()
            .collect()
    };
    assert_eq!(
        of(["pif1", "pif2"]),
        [
            "pif1 Initial 3 not consumed",
            "pif2 Initial 3 consumed",
            "pif2 Main 3 consumed",
            "pif1 Main 3 consumed",
        ]
    );
    assert_eq!(
        of(["pif3", "pif4"]),
        [
            "pif3 Initial 5 not consumed",
            "pif4 Initial 5 consumed",
            "pif4 Main 5 consumed",
            "pif3 Main 5 consumed",
        ]
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, dispatchChanges_dispatchUnchangedPosition_flagEnabled
/// A held finger's move to where it is is still told (Compose's default
/// `isTriggerMoveEventsWhenLocationHasNotChangedEnabled`; compose-rs has no other way).
#[test]
fn a_move_to_the_same_place_is_told() {
    let (mut host, record) = recorded(|record| {
        Leaf(Modifier.size(50.dp()).then_custom(Recorder("pif", record.clone())));
    });
    host.send_pointer(finger_down(13, 0.0, 0.0).at_time(1_000_000));
    record.borrow_mut().clear();
    host.send_pointer(finger(13, 0.0, 0.0).at_time(2_000_000));
    assert_eq!(kinds(&record), ["pif Initial Move", "pif Main Move", "pif Final Move"]);
}

/// Each node's pass and the event's kind, as heard.
fn kinds(record: &Record) -> Vec<String> {
    record
        .borrow()
        .iter()
        .map(|it| format!("{} {:?} {:?}", it.node, it.pass, it.event.kind))
        .collect()
}

fn hover(x: f32, y: f32) -> PointerEvent {
    PointerEvent::moved(Offset::new(x, y), false)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, addHitPath_hoverMove_noChange
/// A mouse coming over three nodes of a path: each enters, down, up and down the path; hovering
/// at the same place again is a move to each (`isTriggerMoveEventsWhenLocationHasNotChangedEnabled`,
/// Compose's default).
#[test]
fn a_hover_enters_a_path_and_the_same_place_again_moves() {
    let (mut host, record) = recorded(|record| {
        Leaf(
            Modifier
                .size(50.dp())
                .then_custom(Recorder("pif1", record.clone()))
                .then_custom(Recorder("pif2", record.clone()))
                .then_custom(Recorder("pif3", record.clone())),
        );
    });
    host.send_pointer(hover(10.0, 10.0));
    let all = |kind: &str| -> Vec<String> {
        [
            "pif1 Initial",
            "pif2 Initial",
            "pif3 Initial",
            "pif3 Main",
            "pif2 Main",
            "pif1 Main",
            "pif1 Final",
            "pif2 Final",
            "pif3 Final",
        ]
        .iter()
        .map(|it| format!("{it} {kind}"))
        .collect()
    };
    assert_eq!(kinds(&record), all("Enter"));
    record.borrow_mut().clear();
    host.send_pointer(hover(10.0, 10.0));
    assert_eq!(kinds(&record), all("Move"));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, addHitPath_hoverMove_enterExit
/// A mouse moving from one child of a parent to the other: the parent hears a move, the child
/// left an exit, the child come to an enter, each branch's initial and main passes in turn.
#[test]
fn a_hover_from_one_child_to_another_exits_one_and_enters_the_other() {
    let (mut host, record) = recorded(|record| {
        Row!(modifier = Modifier.then_custom(Recorder("pif1", record.clone())), {
            Leaf(Modifier.size(50.dp()).then_custom(Recorder("pif2", record.clone())));
            Leaf(Modifier.size(50.dp()).then_custom(Recorder("pif3", record.clone())));
        });
    });
    host.send_pointer(hover(10.0, 10.0));
    assert_eq!(
        kinds(&record),
        [
            "pif1 Initial Enter",
            "pif2 Initial Enter",
            "pif2 Main Enter",
            "pif1 Main Enter",
            "pif1 Final Enter",
            "pif2 Final Enter",
        ]
    );
    record.borrow_mut().clear();
    host.send_pointer(hover(60.0, 10.0));
    assert_eq!(
        kinds(&record),
        [
            "pif1 Initial Move",
            "pif2 Initial Exit",
            "pif2 Main Exit",
            "pif3 Initial Enter",
            "pif3 Main Enter",
            "pif1 Main Move",
            "pif1 Final Move",
            "pif2 Final Exit",
            "pif3 Final Enter",
        ]
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, addHitPath_hoverExit
/// A mouse leaving the window: each node of the path it was over exits.
#[test]
fn a_hover_leaving_exits_each_node_of_its_path() {
    let (mut host, record) = recorded(|record| {
        Leaf(
            Modifier
                .size(50.dp())
                .then_custom(Recorder("pif1", record.clone()))
                .then_custom(Recorder("pif2", record.clone()))
                .then_custom(Recorder("pif3", record.clone())),
        );
    });
    host.send_pointer(hover(10.0, 10.0));
    record.borrow_mut().clear();
    host.send_pointer_left();
    assert_eq!(
        kinds(&record),
        [
            "pif1 Initial Exit",
            "pif2 Initial Exit",
            "pif3 Initial Exit",
            "pif3 Main Exit",
            "pif2 Main Exit",
            "pif1 Main Exit",
            "pif1 Final Exit",
            "pif2 Final Exit",
            "pif3 Final Exit",
        ]
    );
}

/// Writes down "<name> cancel" when cancelled, and for each event in the main pass "<name> down"
/// if a pointer of it comes down, else "<name> moved".
struct CancelLog(&'static str, Rc<RefCell<Vec<String>>>);

struct CancelLogNode(&'static str, Rc<RefCell<Vec<String>>>);

impl ModifierNodeElement for CancelLog {
    type Node = CancelLogNode;
    fn create(&self) -> CancelLogNode {
        CancelLogNode(self.0, self.1.clone())
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
            let down = event.changes.iter().any(|it| it.changed_to_down_ignore_consumed());
            let what = if down { "down" } else { "moved" };
            self.1.borrow_mut().push(format!("{} {what}", self.0));
        }
    }
    fn on_cancel_pointer_input(&mut self) {
        self.1.borrow_mut().push(format!("{} cancel", self.0));
    }
}

/// Each layout's nodes, outer first, in a row: the cancels and what's heard, and the host.
fn cancel_logs(rows: &'static [&'static [&'static str]]) -> (UiHost, Rc<RefCell<Vec<String>>>) {
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = log.clone();
    let host = host(move || {
        let leaves: Vec<Modifier> = rows
            .iter()
            .map(|names| {
                names.iter().fold(Modifier.size(50.dp()), |modifier, name| {
                    modifier.then_custom(CancelLog(name, seen.clone()))
                })
            })
            .collect();
        Row!({
            for leaf in &leaves {
                Leaf(leaf.clone());
            }
        });
    });
    (host, log)
}

fn cancels(log: &Rc<RefCell<Vec<String>>>) -> Vec<String> {
    log.borrow().iter().filter(|line| line.ends_with("cancel")).cloned().collect()
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, processCancel_nothingTracked_doesNotCrash
#[test]
fn a_cancel_with_no_pointer_down_tells_no_node() {
    let (host, log) = cancel_logs(&[&["pif"]]);
    host.with_tree(|tree| tree.cancel_pointers());
    assert!(log.borrow().is_empty());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, processCancel_singlePin_cancelHandlerIsCalled, processCancel_3Pins_cancelHandlersCalledOnceInOrder, processCancel_singlePin_cleared, processCancel_3Pins_cleared
/// A cancel tells each node of the path once, innermost first; nothing of the gesture is kept:
/// the pointer's next event is a new press (Compose's processor hit tests a pointer it doesn't
/// know as coming down).
#[test]
fn a_cancel_tells_a_path_innermost_first_and_keeps_nothing() {
    let (mut host, log) = cancel_logs(&[&["parent", "middle", "child"]]);
    host.send_pointer(finger_down(3, 10.0, 10.0));
    log.borrow_mut().clear();
    host.with_tree(|tree| tree.cancel_pointers());
    assert_eq!(*log.borrow(), ["child cancel", "middle cancel", "parent cancel"]);
    log.borrow_mut().clear();
    host.send_pointer(finger(3, 12.0, 10.0));
    assert_eq!(*log.borrow(), ["child down", "middle down", "parent down"]);

    let (mut host, log) = cancel_logs(&[&["pif"]]);
    host.send_pointer(finger_down(3, 10.0, 10.0));
    host.with_tree(|tree| tree.cancel_pointers());
    assert_eq!(cancels(&log), ["pif cancel"]);

    // One pointer's cancel, the same order.
    let (mut host, log) = cancel_logs(&[&["parent", "middle", "child"]]);
    host.send_pointer(finger_down(3, 10.0, 10.0));
    host.with_tree(|tree| tree.cancel_pointer(PointerId(3)));
    assert_eq!(cancels(&log), ["child cancel", "middle cancel", "parent cancel"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, processCancel_2IndependentPathsFromRoot_cancelHandlersCalledOnceInOrder, processCancel_2IndependentPathsFromRoot_cleared
/// Two independent paths: each path's nodes cancelled innermost first, a path at a time; both
/// pointers' next events are new presses.
#[test]
fn a_cancel_tells_two_independent_paths_one_after_the_other() {
    let (mut host, log) = cancel_logs(&[&["parent1", "child1"], &["parent2", "child2"]]);
    host.send_pointers(&[finger_down(3, 10.0, 10.0), finger_down(5, 60.0, 10.0)]);
    log.borrow_mut().clear();
    host.with_tree(|tree| tree.cancel_pointers());
    assert_eq!(
        *log.borrow(),
        ["child1 cancel", "parent1 cancel", "child2 cancel", "parent2 cancel"]
    );
    log.borrow_mut().clear();
    host.send_pointers(&[finger(3, 12.0, 10.0), finger(5, 62.0, 10.0)]);
    assert_eq!(*log.borrow(), ["child1 down", "parent1 down", "child2 down", "parent2 down"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, processCancel_2BranchingPaths_cancelHandlersCalledOnceInOrder, processCancel_2BranchingPaths_cleared
/// Two paths sharing a parent: the children, then the parent once; both pointers' next events
/// are new presses.
#[test]
fn a_cancel_tells_a_shared_parent_once_after_its_children() {
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = log.clone();
    let mut host = host(move || {
        let parent = Modifier.then_custom(CancelLog("parent", seen.clone()));
        let child1 = Modifier.size(50.dp()).then_custom(CancelLog("child1", seen.clone()));
        let child2 = Modifier.size(50.dp()).then_custom(CancelLog("child2", seen.clone()));
        Row!(modifier = parent, {
            Leaf(child1.clone());
            Leaf(child2.clone());
        });
    });
    host.send_pointers(&[finger_down(3, 10.0, 10.0), finger_down(5, 60.0, 10.0)]);
    log.borrow_mut().clear();
    host.with_tree(|tree| tree.cancel_pointers());
    assert_eq!(*log.borrow(), ["child1 cancel", "child2 cancel", "parent cancel"]);
    log.borrow_mut().clear();
    host.send_pointers(&[finger(3, 12.0, 10.0), finger(5, 62.0, 10.0)]);
    assert_eq!(*log.borrow(), ["child1 down", "child2 down", "parent down"]);
}

/// Keyed by slot type so a chain change matches kept entries to themselves rather than by position.
struct NamedTrace<const SLOT: usize = 0>(&'static str, Rc<RefCell<Vec<String>>>);

struct NamedTraceNode(&'static str, Rc<RefCell<Vec<String>>>);

impl<const SLOT: usize> ModifierNodeElement for NamedTrace<SLOT> {
    type Node = NamedTraceNode;
    fn create(&self) -> NamedTraceNode {
        NamedTraceNode(self.0, self.1.clone())
    }
    fn update(&self, node: &mut NamedTraceNode) {
        node.0 = self.0;
    }
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl ModifierNode for NamedTraceNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
        Some(self)
    }
}

impl PointerInputModifierNode for NamedTraceNode {
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
            let mut pointers = ids(event);
            pointers.sort_unstable();
            self.1.borrow_mut().push(format!("{} {:?} {:?}", self.0, event.kind, pointers));
        }
    }
    fn on_cancel_pointer_input(&mut self) {
        self.1.borrow_mut().push(format!("{} cancel", self.0));
    }
}

fn trace_mod(name: &'static str, log: &Rc<RefCell<Vec<String>>>) -> Modifier {
    Modifier.then_custom(NamedTrace::<0>(name, log.clone()))
}

fn slot_trace_mod<const SLOT: usize>(
    name: &'static str,
    log: &Rc<RefCell<Vec<String>>>,
) -> Modifier {
    Modifier.then_custom(NamedTrace::<SLOT>(name, log.clone()))
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, addPointerInputFilters_allInDetachedState_notAdded
/// Detached branches aren't hit, added to any path, or told of a cancel.
#[test]
fn detached_branches_are_neither_added_nor_cancelled() {
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let attached = mutable_state_of(true);
    let handle = attached.handle();
    let seen = log.clone();
    let mut host = host(move || {
        let log = seen.clone();
        if handle.get() {
            let inner = log.clone();
            Row!(modifier = trace_mod("root", &log), {
                Leaf(
                    Modifier
                        .size(30.dp())
                        .then(trace_mod("mid1", &inner))
                        .then(trace_mod("leaf1", &inner)),
                );
                Leaf(
                    Modifier
                        .size(30.dp())
                        .then(trace_mod("mid2", &inner))
                        .then(trace_mod("leaf2", &inner)),
                );
                Leaf(
                    Modifier
                        .size(30.dp())
                        .then(trace_mod("mid3", &inner))
                        .then(trace_mod("leaf3", &inner)),
                );
            });
        }
    });
    attached.set(false);
    host.frame();
    log.borrow_mut().clear();
    host.send_pointers(&[
        finger_down(3, 10.0, 10.0),
        finger_down(5, 40.0, 10.0),
        finger_down(7, 70.0, 10.0),
    ]);
    host.with_tree(|tree| tree.cancel_pointers());
    assert!(log.borrow().is_empty(), "{:?}", log.borrow());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, addHitPath_emptyHitResult_resultIsCorrect, addHitPath_existingNonMatchingTree_resultIsCorrect, addHitPath_completeMatchingTree_resultIsCorrect, addHitPath_partiallyMatchingTree_resultIsCorrect
/// Adding hit paths into an empty, non-matching, completely matching, or partially matching hit
/// tree merges shared prefixes and keeps diverging tails distinct.
#[test]
fn adding_hit_paths_builds_independent_shared_and_branching_trees() {
    // Empty -> one 3-node path; then a second pointer on the complete matching path.
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = log.clone();
    let mut h = host(move || {
        Leaf(
            Modifier
                .size(50.dp())
                .then(trace_mod("pif1", &seen))
                .then(trace_mod("pif2", &seen))
                .then(trace_mod("pif3", &seen)),
        );
    });
    h.send_pointer(finger_down(1, 10.0, 10.0));
    assert_eq!(*log.borrow(), ["pif3 Press [1]", "pif2 Press [1]", "pif1 Press [1]"]);
    log.borrow_mut().clear();
    h.send_pointer(finger_down(2, 20.0, 10.0));
    assert_eq!(*log.borrow(), ["pif3 Press [1, 2]", "pif2 Press [1, 2]", "pif1 Press [1, 2]"]);

    // Non-matching tree: two independent 3-node paths.
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = log.clone();
    let mut h = host(move || {
        let seen = seen.clone();
        Row!({
            Leaf(
                Modifier
                    .size(50.dp())
                    .then(trace_mod("pif1", &seen))
                    .then(trace_mod("pif2", &seen))
                    .then(trace_mod("pif3", &seen)),
            );
            Leaf(
                Modifier
                    .size(50.dp())
                    .then(trace_mod("pif4", &seen))
                    .then(trace_mod("pif5", &seen))
                    .then(trace_mod("pif6", &seen)),
            );
        });
    });
    h.send_pointer(finger_down(1, 10.0, 10.0));
    log.borrow_mut().clear();
    h.send_pointer(finger_down(2, 60.0, 10.0));
    assert_eq!(
        *log.borrow(),
        [
            "pif3 Press [1]",
            "pif2 Press [1]",
            "pif1 Press [1]",
            "pif6 Press [2]",
            "pif5 Press [2]",
            "pif4 Press [2]",
        ]
    );

    // Partially matching tree: shared `pif1`, then `pif2 -> pif3` vs `pif4 -> pif5`.
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = log.clone();
    let mut h = host(move || {
        let inner = seen.clone();
        Row!(modifier = trace_mod("pif1", &seen), {
            Leaf(
                Modifier
                    .size(50.dp())
                    .then(trace_mod("pif2", &inner))
                    .then(trace_mod("pif3", &inner)),
            );
            Leaf(
                Modifier
                    .size(50.dp())
                    .then(trace_mod("pif4", &inner))
                    .then(trace_mod("pif5", &inner)),
            );
        });
    });
    h.send_pointer(finger_down(1, 10.0, 10.0));
    log.borrow_mut().clear();
    h.send_pointer(finger_down(2, 60.0, 10.0));
    assert_eq!(
        *log.borrow(),
        [
            "pif3 Press [1]",
            "pif2 Press [1]",
            "pif5 Press [2]",
            "pif4 Press [2]",
            "pif1 Press [1, 2]",
        ]
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, addHitPath_dynamicNodeAddedBelowPartiallyMatchingTreeWithOnePointerId_correctResult, addHitPath_dynamicNodeAddedBelowPartiallyMatchingTreeWithTwoPointerIds_correctResult
/// A node dynamically added at the bottom of an existing hover path merges into the prefix
/// without duplicating any node on that path or disturbing a second pointer's path.
#[test]
fn a_node_added_below_a_tracked_hover_path_enters_without_duplicating_the_prefix() {
    for two_pointers in [false, true] {
        let log: Rc<RefCell<Vec<String>>> = Rc::default();
        let extra = mutable_state_of(false);
        let handle = extra.handle();
        let seen = log.clone();
        let mut host = host(move || {
            let log = seen.clone();
            let add_new = handle.get();
            Row!({
                if two_pointers {
                    Leaf(
                        Modifier
                            .size(50.dp())
                            .then(trace_mod("pif1", &log))
                            .then(trace_mod("pif2", &log))
                            .then(trace_mod("pif3", &log))
                            .then(trace_mod("pif4", &log)),
                    );
                }
                let inner = log.clone();
                Stack!(
                    modifier = Modifier
                        .size(50.dp())
                        .then(trace_mod("pif5", &log))
                        .then(trace_mod("pif6", &log))
                        .then(trace_mod("pif7", &log))
                        .then(trace_mod("pif8", &log)),
                    {
                        if add_new {
                            Leaf(
                                Modifier
                                    .offset(25.dp(), 0.dp())
                                    .size(DpSize::new(25.dp(), 50.dp()))
                                    .then(trace_mod("new", &inner)),
                            );
                        }
                    }
                );
            });
        });
        let hover_x = if two_pointers { 50.0 } else { 0.0 };
        if two_pointers {
            host.send_pointer(finger_down(1, 10.0, 10.0));
        }
        host.send_pointer(hover(hover_x + 5.0, 10.0));
        log.borrow_mut().clear();
        extra.set(true);
        host.frame();
        assert!(log.borrow().is_empty(), "{:?}", log.borrow());

        // Moving onto `new` merges `[pif5..8, new]` onto `[pif5..8]` in `hover_move` (`new` enters,
        // `pif5..8` move once each, and resting pointer `1` moves on `pif1..4` after pointer `0`).
        host.send_pointer(hover(hover_x + 30.0, 10.0));
        let mut expected = vec![
            "new Enter [0]",
            "pif8 Move [0]",
            "pif7 Move [0]",
            "pif6 Move [0]",
            "pif5 Move [0]",
        ];
        if two_pointers {
            expected.extend(["pif4 Move [1]", "pif3 Move [1]", "pif2 Move [1]", "pif1 Move [1]"]);
        }
        assert_eq!(*log.borrow(), expected, "two_pointers={two_pointers}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, removeDetachedPointerInputFilters_complexNothingDetached_nothingRemovedNoCancelsCalled
/// Re-running a frame over five active paths of varying depths and shared parents when nothing
/// detaches cancels nothing and leaves every path intact.
#[test]
fn a_frame_detaching_nothing_cancels_nothing_across_five_paths() {
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let tick = mutable_state_of(0);
    let handle = tick.handle();
    let seen = log.clone();
    let mut host = host(move || {
        let _ = handle.get();
        let log = seen.clone();
        Row!({
            Leaf(Modifier.size(20.dp()).then(trace_mod("pif1", &log)));
            Leaf(
                Modifier.size(20.dp()).then(trace_mod("pif3", &log)).then(trace_mod("pif2", &log)),
            );
            Leaf(
                Modifier
                    .size(20.dp())
                    .then(trace_mod("pif6", &log))
                    .then(trace_mod("pif5", &log))
                    .then(trace_mod("pif4", &log)),
            );
            let inner = log.clone();
            Row!(modifier = trace_mod("pif9", &log), {
                Leaf(Modifier.size(20.dp()).then(trace_mod("pif7", &inner)));
                Leaf(Modifier.size(20.dp()).then(trace_mod("pif8", &inner)));
            });
        });
    });
    host.send_pointers(&[
        finger_down(1, 5.0, 5.0),
        finger_down(2, 25.0, 5.0),
        finger_down(3, 45.0, 5.0),
        finger_down(4, 65.0, 5.0),
        finger_down(5, 85.0, 5.0),
    ]);
    log.borrow_mut().clear();
    tick.set(1);
    host.frame();
    assert!(log.borrow().is_empty(), "{:?}", log.borrow());
    host.send_pointers(&[
        finger(1, 6.0, 5.0),
        finger(2, 26.0, 5.0),
        finger(3, 46.0, 5.0),
        finger(4, 66.0, 5.0),
        finger(5, 86.0, 5.0),
    ]);
    assert_eq!(
        *log.borrow(),
        [
            "pif1 Move [1]",
            "pif2 Move [2]",
            "pif3 Move [2]",
            "pif4 Move [3]",
            "pif5 Move [3]",
            "pif6 Move [3]",
            "pif7 Move [4]",
            "pif8 Move [5]",
            "pif9 Move [4, 5]",
        ]
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, removeDetachedPointerInputFilters_1PathRootDetached_allRemovedAndCorrectCancels, removeDetachedPointerInputFilters_1PathMiddleDetached_removesAndCancelsCorrect, removeDetachedPointerInputFilters_1PathLeafDetached_removesAndCancelsCorrect
/// On a single 3-node path, detaching the root, the middle or the leaf cancels the detached node
/// and what was below it innermost first, keeping any prefix above it alive; tested both for
/// nested layout nodes and for modifier entries on one layout node.
#[test]
fn removing_a_root_middle_or_leaf_on_one_path_cancels_it_and_below_and_keeps_above() {
    let cases: &[(usize, &[&str], &[&str])] = &[
        (0, &["leaf cancel", "middle cancel", "root cancel"], &[]),
        (1, &["leaf cancel", "middle cancel"], &["root Move [1]"]),
        (2, &["leaf cancel"], &["middle Move [1]", "root Move [1]"]),
    ];
    for &(cut_at, expected_cancels, expected_moves) in cases {
        for by_chain in [false, true] {
            let log: Rc<RefCell<Vec<String>>> = Rc::default();
            let keep_from = mutable_state_of(3usize);
            let handle = keep_from.handle();
            let seen = log.clone();
            let mut host = host(move || {
                let depth = handle.get();
                let log = seen.clone();
                if by_chain {
                    // Detaching only `cut_at` leaves any modifier below it attached in the chain,
                    // yet still cuts it off the gesture path and cancels it innermost first.
                    let r = if depth != 0 { slot_trace_mod::<0>("root", &log) } else { Modifier };
                    let m = if depth != 1 { slot_trace_mod::<1>("middle", &log) } else { Modifier };
                    let l = if depth != 2 { slot_trace_mod::<2>("leaf", &log) } else { Modifier };
                    Leaf(Modifier.size(50.dp()).then(r).then(m).then(l));
                } else if depth > 0 {
                    let mid_log = log.clone();
                    Stack!(modifier = Modifier.size(50.dp()).then(trace_mod("root", &log)), {
                        if depth > 1 {
                            let leaf_log = mid_log.clone();
                            Stack!(modifier = trace_mod("middle", &mid_log), {
                                if depth > 2 {
                                    Leaf(Modifier.size(50.dp()).then(trace_mod("leaf", &leaf_log)));
                                }
                            });
                        }
                    });
                }
            });
            host.send_pointer(finger_down(1, 10.0, 10.0));
            log.borrow_mut().clear();
            keep_from.set(cut_at);
            host.frame();
            assert_eq!(*log.borrow(), expected_cancels, "cut={cut_at}, by_chain={by_chain}");
            log.borrow_mut().clear();
            host.send_pointer(finger(1, 12.0, 10.0));
            assert_eq!(*log.borrow(), expected_moves, "cut={cut_at}, by_chain={by_chain}");
        }
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, removeDetachedPointerInputFilters_3Roots1Detached_removesAndCancelsCorrect, removeDetachedPointerInputFilters_3Roots1MiddleDetached_removesAndCancelsCorrect, removeDetachedPointerInputFilters_3Roots1LeafDetached_removesAndCancelsCorrect, removeDetachedPointerInputFilters_3Roots2Detached_removesAndCancelsCorrect, removeDetachedPointerInputFilters_3Roots2MiddlesDetached_removesAndCancelsCorrect, removeDetachedPointerInputFilters_3Roots2LeafsDetached_removesAndCancelsCorrect, removeDetachedPointerInputFilters_3Roots3Detached_allRemovedAndCancelsCorrect, removeDetachedPointerInputFilters_3Roots3MiddlesDetached_removesAndCancelsCorrect, removeDetachedPointerInputFilters_3Roots3LeafsDetached_removesAndCancelsCorrect, removeDetachedPointerInputFilters_3RootsStaggeredDetached_removesAndCancelsCorrect
/// Three independent 3-node branches: detaching 1, 2 or 3 roots, middles or leaves (or a
/// staggered cut across the three) cancels each removed suffix innermost first and keeps the
/// surviving prefixes receiving their pointers' moves.
#[test]
fn removing_detached_nodes_across_three_independent_roots_cancels_each_and_keeps_the_prefixes() {
    // Each case gives the surviving depth (0..=3) of branches 1, 2 and 3.
    let depths: [(usize, usize, usize); 10] = [
        (3, 3, 0), // 3Roots1Detached (branch 3)
        (1, 3, 3), // 3Roots1MiddleDetached (branch 1)
        (3, 2, 3), // 3Roots1LeafDetached (branch 2)
        (0, 3, 0), // 3Roots2Detached (branches 1, 3)
        (1, 1, 3), // 3Roots2MiddlesDetached (branches 1, 2)
        (3, 2, 2), // 3Roots2LeafsDetached (branches 2, 3)
        (0, 0, 0), // 3Roots3Detached
        (1, 1, 1), // 3Roots3MiddlesDetached
        (2, 2, 2), // 3Roots3LeafsDetached
        (0, 1, 2), // 3RootsStaggeredDetached
    ];
    let names = [
        ["root1", "middle1", "leaf1"],
        ["root2", "middle2", "leaf2"],
        ["root3", "middle3", "leaf3"],
    ];
    let pids = [3u64, 5, 7];
    for (d1, d2, d3) in depths {
        let log: Rc<RefCell<Vec<String>>> = Rc::default();
        let kept = mutable_state_of([3usize, 3, 3]);
        let handle = kept.handle();
        let seen = log.clone();
        let mut host = host(move || {
            let cur = handle.get();
            let log = seen.clone();
            Row!({
                for b in 0..3 {
                    let d = cur[b];
                    let r = if d > 0 { trace_mod(names[b][0], &log) } else { Modifier };
                    let m = if d > 1 { trace_mod(names[b][1], &log) } else { Modifier };
                    let l = if d > 2 { trace_mod(names[b][2], &log) } else { Modifier };
                    Leaf(Modifier.size(30.dp()).then(r).then(m).then(l));
                }
            });
        });
        host.send_pointers(&[
            finger_down(3, 10.0, 10.0),
            finger_down(5, 40.0, 10.0),
            finger_down(7, 70.0, 10.0),
        ]);
        log.borrow_mut().clear();
        let target = [d1, d2, d3];
        kept.set(target);
        host.frame();

        let mut expected_cancels = Vec::new();
        let mut expected_moves = Vec::new();
        for b in 0..3 {
            for level in (target[b]..3).rev() {
                expected_cancels.push(format!("{} cancel", names[b][level]));
            }
            for level in (0..target[b]).rev() {
                expected_moves.push(format!("{} Move [{}]", names[b][level], pids[b]));
            }
        }
        assert_eq!(*log.borrow(), expected_cancels, "target={target:?}");
        log.borrow_mut().clear();
        host.send_pointers(&[finger(3, 12.0, 10.0), finger(5, 42.0, 10.0), finger(7, 72.0, 10.0)]);
        assert_eq!(*log.borrow(), expected_moves, "target={target:?}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, removeDetachedPointerInputFilters_rootWith3MiddlesDetached_allRemovedAndCorrectCancels, removeDetachedPointerInputFilters_rootWith3Middles1Detached_removesAndCancelsCorrect, removeDetachedPointerInputFilters_rootWith3Middles2Detached_removesAndCancelsCorrect, removeDetachedPointerInputFilters_rootWith3MiddlesAllDetached_allMiddlesRemoved
/// One shared root above three `middle -> leaf` branches: detaching 1, 2 or all 3 middles (or the
/// root itself) cancels the removed subtrees innermost first and keeps the remaining branches and
/// root alive.
#[test]
fn removing_middles_or_the_shared_root_of_three_branches_cancels_in_order_and_keeps_the_rest() {
    // (keep_root, [keep_b1, keep_b2, keep_b3])
    let cases = [
        (false, [false, false, false]), // rootWith3MiddlesDetached
        (true, [true, true, false]),    // rootWith3Middles1Detached
        (true, [false, false, true]),   // rootWith3Middles2Detached
        (true, [false, false, false]),  // rootWith3MiddlesAllDetached
    ];
    let branches = [("middle1", "leaf1", 3u64), ("middle2", "leaf2", 5), ("middle3", "leaf3", 7)];
    for (keep_root, keep_branches) in cases {
        let log: Rc<RefCell<Vec<String>>> = Rc::default();
        let state = mutable_state_of((true, [true, true, true]));
        let handle = state.handle();
        let seen = log.clone();
        let mut host = host(move || {
            let (root_on, br_on) = handle.get();
            let log = seen.clone();
            if root_on {
                let inner = log.clone();
                Row!(
                    modifier =
                        Modifier.size(DpSize::new(90.dp(), 30.dp())).then(trace_mod("root", &log)),
                    {
                        for b in 0..3 {
                            let m = if br_on[b] {
                                trace_mod(branches[b].0, &inner)
                                    .then(trace_mod(branches[b].1, &inner))
                            } else {
                                Modifier
                            };
                            Leaf(Modifier.size(30.dp()).then(m));
                        }
                    }
                );
            }
        });
        host.send_pointers(&[
            finger_down(3, 10.0, 10.0),
            finger_down(5, 40.0, 10.0),
            finger_down(7, 70.0, 10.0),
        ]);
        log.borrow_mut().clear();
        state.set((keep_root, keep_branches));
        host.frame();

        let mut expected_cancels = Vec::new();
        for b in 0..3 {
            if !keep_root || !keep_branches[b] {
                expected_cancels.push(format!("{} cancel", branches[b].1));
                expected_cancels.push(format!("{} cancel", branches[b].0));
            }
        }
        if !keep_root {
            expected_cancels.push("root cancel".into());
        }
        assert_eq!(*log.borrow(), expected_cancels, "case=({keep_root}, {keep_branches:?})");

        log.borrow_mut().clear();
        host.send_pointers(&[finger(3, 12.0, 10.0), finger(5, 42.0, 10.0), finger(7, 72.0, 10.0)]);
        let mut expected_moves = Vec::new();
        if keep_root {
            for b in 0..3 {
                if keep_branches[b] {
                    expected_moves.push(format!("{} Move [{}]", branches[b].1, branches[b].2));
                    expected_moves.push(format!("{} Move [{}]", branches[b].0, branches[b].2));
                }
            }
            expected_moves.push("root Move [3, 5, 7]".into());
        }
        assert_eq!(*log.borrow(), expected_moves, "case=({keep_root}, {keep_branches:?})");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, removeDetachedPointerInputFilters_middleWith3Leafs1Detached_correctLeafRemoved, removeDetachedPointerInputFilters_middleWith3Leafs2Detached_correctLeafsRemoved, removeDetachedPointerInputFilters_middleWith3LeafsAllDetached_allLeafsRemoved
/// Shared `root -> middle` above three leaves: detaching 1, 2 or all 3 leaves cancels only the
/// detached leaves and keeps `root -> middle` (and any staying leaf) receiving all three pointers.
#[test]
fn removing_leaves_under_a_shared_middle_cancels_those_leaves_and_keeps_the_shared_stem() {
    let cases = [
        [true, false, true],   // middleWith3Leafs1Detached (leaf2)
        [false, true, false],  // middleWith3Leafs2Detached (leaf1, leaf3)
        [false, false, false], // middleWith3LeafsAllDetached
    ];
    let leaves = [("leaf1", 3u64), ("leaf2", 5), ("leaf3", 7)];
    for keep_leaves in cases {
        let log: Rc<RefCell<Vec<String>>> = Rc::default();
        let state = mutable_state_of([true, true, true]);
        let handle = state.handle();
        let seen = log.clone();
        let mut host = host(move || {
            let on = handle.get();
            let log = seen.clone();
            let inner = log.clone();
            Row!(
                modifier = Modifier
                    .size(DpSize::new(90.dp(), 30.dp()))
                    .then(trace_mod("root", &log))
                    .then(trace_mod("middle", &log)),
                {
                    for b in 0..3 {
                        let l = if on[b] { trace_mod(leaves[b].0, &inner) } else { Modifier };
                        Leaf(Modifier.size(30.dp()).then(l));
                    }
                }
            );
        });
        host.send_pointers(&[
            finger_down(3, 10.0, 10.0),
            finger_down(5, 40.0, 10.0),
            finger_down(7, 70.0, 10.0),
        ]);
        log.borrow_mut().clear();
        state.set(keep_leaves);
        host.frame();

        let expected_cancels: Vec<String> = (0..3)
            .filter(|&b| !keep_leaves[b])
            .map(|b| format!("{} cancel", leaves[b].0))
            .collect();
        assert_eq!(*log.borrow(), expected_cancels, "keep={keep_leaves:?}");

        log.borrow_mut().clear();
        host.send_pointers(&[finger(3, 12.0, 10.0), finger(5, 42.0, 10.0), finger(7, 72.0, 10.0)]);
        let mut expected_moves: Vec<String> = (0..3)
            .filter(|&b| keep_leaves[b])
            .map(|b| format!("{} Move [{}]", leaves[b].0, leaves[b].1))
            .collect();
        expected_moves.push("middle Move [3, 5, 7]".into());
        expected_moves.push("root Move [3, 5, 7]".into());
        assert_eq!(*log.borrow(), expected_moves, "keep={keep_leaves:?}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, removeHitPath_onePathPointerIdRemoved_hitTestResultIsEmpty, removeHitPath_onePathOtherPointerIdRemoved_hitTestResultIsNotChanged, removeHitPath_2IndependentPaths1PointerIdRemoved_resultContainsRemainingPath, removeHitPath_2PathsShareNodes1PointerIdRemoved_resultContainsRemainingPath, removeHitPath_2PathsShare2NodesLongPathPointerIdRemoved_resultJustHasShortPath, removeHitPath_2PathsShare2NodesShortPathPointerIdRemoved_resultJustHasLongPath, removeHitPath_2PathsShare1NodeLongPathPointerIdRemoved_resultJustHasShortPath, removeHitPath_2PathsShare1NodeShortPathPointerIdRemoved_resultJustHasLongPath
/// Releasing a pointer drops its unshared nodes from the hit paths while leaving every other
/// pointer's path — independent, identical, longer or shorter — intact.
#[test]
fn releasing_a_pointer_prunes_its_unshared_nodes_and_keeps_other_pointers_paths() {
    // 1 & 2: one path: removing an unrelated pointer keeps it; removing its own clears it.
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = log.clone();
    let mut h = host(move || {
        let seen = seen.clone();
        Row!({
            Leaf(
                Modifier
                    .size(50.dp())
                    .then(trace_mod("root", &seen))
                    .then(trace_mod("middle", &seen))
                    .then(trace_mod("leaf", &seen)),
            );
        });
    });
    h.send_pointer(finger_down(3, 10.0, 10.0));
    h.send_pointer(finger_down(99, 80.0, 10.0));
    h.send_pointer(finger_up(99, 80.0, 10.0));
    log.borrow_mut().clear();
    h.send_pointer(finger(3, 12.0, 10.0));
    assert_eq!(*log.borrow(), ["leaf Move [3]", "middle Move [3]", "root Move [3]"]);
    h.send_pointer(finger_up(3, 12.0, 10.0));
    log.borrow_mut().clear();
    h.with_tree(|tree| tree.cancel_pointers());
    assert!(log.borrow().is_empty());

    // 3: two independent paths, pointer 5 released -> pointer 3's path remains.
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = log.clone();
    let mut h = host(move || {
        let seen = seen.clone();
        Row!({
            Leaf(
                Modifier
                    .size(50.dp())
                    .then(trace_mod("r1", &seen))
                    .then(trace_mod("m1", &seen))
                    .then(trace_mod("l1", &seen)),
            );
            Leaf(
                Modifier
                    .size(50.dp())
                    .then(trace_mod("r2", &seen))
                    .then(trace_mod("m2", &seen))
                    .then(trace_mod("l2", &seen)),
            );
        });
    });
    h.send_pointers(&[finger_down(3, 10.0, 10.0), finger_down(5, 60.0, 10.0)]);
    h.send_pointers(&[finger(3, 10.0, 10.0), finger_up(5, 60.0, 10.0)]);
    log.borrow_mut().clear();
    h.send_pointer(finger(3, 12.0, 10.0));
    assert_eq!(*log.borrow(), ["l1 Move [3]", "m1 Move [3]", "r1 Move [3]"]);

    // 4: two pointers sharing all 3 nodes (`root -> middle -> leaf`), pointer 3 released.
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = log.clone();
    let mut h = host(move || {
        Leaf(
            Modifier
                .size(50.dp())
                .then(trace_mod("root", &seen))
                .then(trace_mod("middle", &seen))
                .then(trace_mod("leaf", &seen)),
        );
    });
    h.send_pointers(&[finger_down(3, 10.0, 10.0), finger_down(5, 20.0, 10.0)]);
    h.send_pointers(&[finger_up(3, 10.0, 10.0), finger(5, 20.0, 10.0)]);
    log.borrow_mut().clear();
    h.send_pointer(finger(5, 22.0, 10.0));
    assert_eq!(*log.borrow(), ["leaf Move [5]", "middle Move [5]", "root Move [5]"]);

    // 5 & 6: 2 shared nodes (`root -> middle`), pointer 3 also on `leaf` (x < 30), pointer 5 on
    // `middle` outside `leaf` (30 <= x < 60).
    for release_long in [true, false] {
        let log: Rc<RefCell<Vec<String>>> = Rc::default();
        let seen = log.clone();
        let mut h = host(move || {
            let inner = seen.clone();
            Stack!(
                modifier = Modifier
                    .size(DpSize::new(60.dp(), 30.dp()))
                    .then(trace_mod("root", &seen))
                    .then(trace_mod("middle", &seen)),
                {
                    Leaf(Modifier.size(30.dp()).then(trace_mod("leaf", &inner)));
                }
            );
        });
        h.send_pointers(&[finger_down(3, 10.0, 10.0), finger_down(5, 45.0, 10.0)]);
        if release_long {
            h.send_pointers(&[finger_up(3, 10.0, 10.0), finger(5, 45.0, 10.0)]);
            log.borrow_mut().clear();
            h.send_pointer(finger(5, 47.0, 10.0));
            assert_eq!(*log.borrow(), ["middle Move [5]", "root Move [5]"]);
        } else {
            h.send_pointers(&[finger(3, 10.0, 10.0), finger_up(5, 45.0, 10.0)]);
            log.borrow_mut().clear();
            h.send_pointer(finger(3, 12.0, 10.0));
            assert_eq!(*log.borrow(), ["leaf Move [3]", "middle Move [3]", "root Move [3]"]);
        }
    }

    // 7 & 8: 1 shared node (`root`), pointer 3 on `root -> middle -> leaf` (x < 30), pointer 5 on
    // `root` alone (30 <= x < 60).
    for release_long in [true, false] {
        let log: Rc<RefCell<Vec<String>>> = Rc::default();
        let seen = log.clone();
        let mut h = host(move || {
            let inner = seen.clone();
            Stack!(
                modifier =
                    Modifier.size(DpSize::new(60.dp(), 30.dp())).then(trace_mod("root", &seen)),
                {
                    Leaf(
                        Modifier
                            .size(30.dp())
                            .then(trace_mod("middle", &inner))
                            .then(trace_mod("leaf", &inner)),
                    );
                }
            );
        });
        h.send_pointers(&[finger_down(3, 10.0, 10.0), finger_down(5, 45.0, 10.0)]);
        if release_long {
            h.send_pointers(&[finger_up(3, 10.0, 10.0), finger(5, 45.0, 10.0)]);
            log.borrow_mut().clear();
            h.send_pointer(finger(5, 47.0, 10.0));
            assert_eq!(*log.borrow(), ["root Move [5]"]);
        } else {
            h.send_pointers(&[finger(3, 10.0, 10.0), finger_up(5, 45.0, 10.0)]);
            log.borrow_mut().clear();
            h.send_pointer(finger(3, 12.0, 10.0));
            assert_eq!(*log.borrow(), ["leaf Move [3]", "middle Move [3]", "root Move [3]"]);
        }
    }
}

/// A child node left unplaced mid-gesture (still attached in the tree) is skipped by captured
/// pointer delivery while its placed parent keeps hearing the gesture.
#[test]
fn an_unplaced_node_on_a_captured_path_is_skipped_without_disturbing_its_parent() {
    #[derive(PartialEq)]
    struct PlaceIf(bool);
    impl MeasurePolicy for PlaceIf {
        fn measure(
            &self,
            scope: &mut MeasureScope<'_>,
            measurables: &[Measurable],
            constraints: Constraints,
        ) -> MeasureResult {
            let placeable = scope.measure(measurables[0], constraints);
            if self.0 {
                scope.place(&placeable, 0, 0);
            }
            scope.layout(placeable.width, placeable.height)
        }
        fn same(&self, other: &dyn std::any::Any) -> bool {
            other.downcast_ref::<Self>() == Some(self)
        }
    }

    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let place = mutable_state_of(true);
    let handle = place.handle();
    let seen = log.clone();
    let mut host = host(move || {
        let on = handle.get();
        let inner = seen.clone();
        emit_layout_node(
            Modifier.size(50.dp()).then(trace_mod("parent", &seen)),
            Rc::new(PlaceIf(on)),
            || {
                Leaf(Modifier.size(50.dp()).then(trace_mod("child", &inner)));
            },
        );
    });
    host.send_pointer(PointerEvent::press(Offset::new(10.0, 10.0)));
    log.borrow_mut().clear();
    place.set(false);
    host.frame();
    log.borrow_mut().clear();
    host.send_pointer(PointerEvent::moved(Offset::new(12.0, 10.0), true));
    assert_eq!(*log.borrow(), ["parent Move [0]"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, trackpadPan_preservesPanEvents_whenNodeExitsBounds, trackpadPan_hoverTransitionToHoverAndMove
/// A hovered node scrolling out of bounds during a trackpad pan hears `Exit` and still hears the
/// pan to its end; once the pan ends, hover transitions cleanly to the new node under the cursor.
#[test]
fn a_trackpad_pan_exits_a_scrolled_node_keeps_its_pan_and_transitions_back_to_hover() {
    let record = Record::default();
    let scrolled = mutable_state_of(false);
    let handle = scrolled.handle();
    let seen = record.clone();
    let mut host = host(move || {
        let away = handle.get();
        let seen = seen.clone();
        let x = if away { 60.dp() } else { 0.dp() };
        Stack!(modifier = Modifier.size(100.dp()), {
            Leaf(Modifier.size(50.dp()).then_custom(Recorder("child2", seen.clone())));
            Leaf(Modifier.offset(x, x).size(50.dp()).then_custom(Recorder("child1", seen.clone())));
        });
    });
    let cursor = Offset::new(10.0, 10.0);
    host.send_pointer(hover(10.0, 10.0));
    assert_eq!(kinds(&record), ["child1 Initial Enter", "child1 Main Enter", "child1 Final Enter"]);
    record.borrow_mut().clear();

    host.send_pointer(PointerEvent::pan(PointerEventType::PanStart, cursor, Offset::ZERO));
    record.borrow_mut().clear();
    scrolled.set(true);
    host.frame();
    host.send_pointer(PointerEvent::pan(PointerEventType::PanMove, cursor, Offset::new(0.0, 10.0)));
    assert_eq!(
        kinds(&record),
        [
            "child1 Initial Exit",
            "child1 Main Exit",
            "child1 Final Exit",
            "child1 Initial PanMove",
            "child1 Main PanMove",
            "child1 Final PanMove",
        ]
    );
    record.borrow_mut().clear();
    host.send_pointer(PointerEvent::pan(PointerEventType::PanEnd, cursor, Offset::ZERO));
    assert_eq!(
        kinds(&record),
        ["child1 Initial PanEnd", "child1 Main PanEnd", "child1 Final PanEnd"]
    );

    // Transition back to hover over `child2`, then move within `child2`.
    record.borrow_mut().clear();
    host.send_pointer(hover(10.0, 10.0));
    assert_eq!(kinds(&record), ["child2 Initial Enter", "child2 Main Enter", "child2 Final Enter"]);
    record.borrow_mut().clear();
    host.send_pointer(hover(20.0, 20.0));
    assert_eq!(kinds(&record), ["child2 Initial Move", "child2 Main Move", "child2 Final Move"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/input/pointer/HitPathTrackerTest.kt, trackpadPan_syntheticHoverConsumption_doesNotMutatePanChanges
/// Consuming the synthetic `Enter` delivered when a `PanStart` begins over an unhovered node does
/// not mark the `PanStart`'s own change consumed.
#[test]
fn consuming_a_pans_synthetic_enter_leaves_its_pan_change_unconsumed() {
    struct ConsumeEnter(Rc<RefCell<Vec<(PointerEventType, bool)>>>);
    impl ModifierNode for ConsumeEnter {
        fn as_pointer_input(&mut self) -> Option<&mut dyn PointerInputModifierNode> {
            Some(self)
        }
    }
    impl PointerInputModifierNode for ConsumeEnter {
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
            if pass == PointerEventPass::Final {
                if event.kind == PointerEventType::Enter {
                    for change in &event.changes {
                        change.consume();
                    }
                }
                self.0.borrow_mut().push((event.kind, event.changes[0].is_consumed()));
            }
        }
    }
    struct ConsumeEnterElement(Rc<RefCell<Vec<(PointerEventType, bool)>>>);
    impl ModifierNodeElement for ConsumeEnterElement {
        type Node = ConsumeEnter;
        fn create(&self) -> ConsumeEnter {
            ConsumeEnter(self.0.clone())
        }
        fn update(&self, _: &mut ConsumeEnter) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }

    let seen: Rc<RefCell<Vec<(PointerEventType, bool)>>> = Rc::default();
    let log = seen.clone();
    let mut host = host(move || {
        Leaf(Modifier.size(100.dp()).then_custom(ConsumeEnterElement(log.clone())));
    });
    host.send_pointer(PointerEvent::pan(
        PointerEventType::PanStart,
        Offset::new(10.0, 10.0),
        Offset::ZERO,
    ));
    assert_eq!(
        *seen.borrow(),
        [(PointerEventType::Enter, true), (PointerEventType::PanStart, false)]
    );
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
    // Then it hovers, whatever's under it: at its next move, as Compose's release hit-tests
    // nothing (D2361).
    assert!(presses(&record, "b").is_empty());
    host.send_pointer(PointerEvent::moved(over_b, false));
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
